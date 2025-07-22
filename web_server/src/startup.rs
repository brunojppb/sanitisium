use std::net::TcpListener;

use actix_web::{App, HttpServer, dev::Server, middleware::Logger, rt::signal, web};
use actix_web_opentelemetry::RequestTracing;
use anyhow::{Context, Result};
use apalis::{
    layers::WorkerBuilderExt,
    prelude::{Monitor, WorkerBuilder, WorkerFactoryFn},
};
use apalis_sql::{sqlite::SqliteStorage, sqlx::SqlitePool};
use futures::future;

use crate::{
    app_settings::AppSettings,
    job::{SanitisePDF, sanitise_pdf},
    routes::{health::health_check, sanitise::enqueue_pdf},
};

pub struct Application {
    port: u16,
    server: Server,
    monitor: Monitor,
}

impl Application {
    pub async fn build(settings: AppSettings) -> Result<Self> {
        let address = format!(
            "{}:{}",
            settings.application.host, settings.application.port
        );
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr().unwrap().port();
        let (server, monitor) = run(listener, settings).await?;
        Ok(Self {
            server,
            port,
            monitor,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Run the web server and blocks the main thread until it stops
    pub async fn run_until_stopped(self) -> Result<()> {
        future::try_join(self.server, self.monitor.run_with_signal(signal::ctrl_c())).await?;
        Ok(())
    }
}

async fn run(listener: TcpListener, settings: AppSettings) -> Result<(Server, Monitor)> {
    let port = listener
        .local_addr()
        .expect("TCPListener is invalid")
        .port();

    let settings = web::Data::new(settings);

    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .with_context(|| "Could not connect to SQLite in-memory")?;
    SqliteStorage::setup(&pool)
        .await
        .expect("Could not run sqlite migrations");

    let pdf_processing_storage: SqliteStorage<SanitisePDF> = SqliteStorage::new(pool);
    let arc_storage = web::Data::new(pdf_processing_storage.clone());

    let server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(RequestTracing::new())
            .route("/management/health", web::get().to(health_check))
            .route("/sanitise/pdf", web::post().to(enqueue_pdf))
            .app_data(settings.clone())
            .app_data(arc_storage.clone())
    })
    .listen(listener)?
    .run();

    let monitor = Monitor::new().register({
        WorkerBuilder::new("pdf-regenerator")
            .enable_tracing()
            .concurrency(10)
            .backend(pdf_processing_storage)
            .build_fn(sanitise_pdf)
    });

    tracing::info!("Sanitisium Web Server is running. port={port}");

    Ok((server, monitor))
}
