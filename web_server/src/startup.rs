use std::{fs, net::TcpListener, sync::Arc};

use actix_web::{
    App, HttpServer,
    dev::Server,
    middleware::Logger,
    web::{self, PayloadConfig},
};
use actix_web_opentelemetry::RequestTracing;
use anyhow::Result;
use futures::future;
use tracing_actix_web::TracingLogger;

use crate::{
    app_settings::AppSettings,
    routes::{health::health_check, sanitise::enqueue_pdf},
    storage::FileStorage,
    workers::job::SanitisePdfScheduler,
};

pub struct Application {
    port: u16,
    server: Server,
    services: Arc<AppServices>,
}

/// Services injected into our Request handlers
/// and other parts of our application
pub struct AppServices {
    pub job_scheduler: Arc<SanitisePdfScheduler>,
    pub file_storage: Arc<FileStorage<String>>,
}

impl Application {
    pub async fn build(settings: AppSettings) -> Result<Self> {
        let address = format!(
            "{}:{}",
            settings.application.host, settings.application.port
        );
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr().unwrap().port();
        let (server, services) = run(listener, settings).await?;
        Ok(Self {
            server,
            port,
            services,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Run the web server and blocks the main thread until it stops
    pub async fn run_until_stopped(self) -> Result<()> {
        future::try_join(self.server, self.services.job_scheduler.run_until_stopped()).await?;
        Ok(())
    }
}

// Allowing max of 50MB file size to be uploaded for now
const MAX_PAYLOAD_SIZE: usize = 1024 * 1024 * 50;

async fn run(listener: TcpListener, settings: AppSettings) -> Result<(Server, Arc<AppServices>)> {
    let port = listener
        .local_addr()
        .expect("TCPListener is invalid")
        .port();

    // Make sure the file sanitisation directory exists
    match fs::exists(&settings.sanitisation.pdfs_dir) {
        Ok(false) => fs::create_dir(&settings.sanitisation.pdfs_dir)
            .expect("Error while creating base dir for file sanitisation"),
        Ok(true) => {}
        Err(error) => {
            let message =
                format!("Could not create file sanitisation base directory. error={error}");
            tracing::error!(message);
            panic!("Cannot startup the server. error={message}");
        }
    };

    let file_storage = FileStorage::new(settings.sanitisation.pdfs_dir.clone());
    let file_storage = Arc::new(file_storage);

    let job_scheduler = SanitisePdfScheduler::build(settings).await?;
    let job_scheduler = Arc::new(job_scheduler);

    let services = AppServices {
        job_scheduler,
        file_storage,
    };

    let arc_services = Arc::new(services);
    let data_arc_services = web::Data::new(arc_services.clone());

    let server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(TracingLogger::default())
            .wrap(RequestTracing::default())
            .route("/management/health", web::get().to(health_check))
            .route("/sanitise/pdf", web::post().to(enqueue_pdf))
            .app_data(data_arc_services.clone())
            .app_data(PayloadConfig::new(MAX_PAYLOAD_SIZE))
    })
    .listen(listener)?
    .run();

    tracing::info!("Sanitisium Web Server is running. port={port}");

    Ok((server, arc_services))
}
