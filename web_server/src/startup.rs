use std::net::TcpListener;

use actix_web::{App, HttpServer, dev::Server, web};

use crate::{app_settings::AppSettings, routes::health::health_check};

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(settings: AppSettings) -> Result<Self, std::io::Error> {
        let address = format!(
            "{}:{}",
            settings.application.host, settings.application.port
        );
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr().unwrap().port();
        let server = run(listener, settings)?;
        Ok(Self { server, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Run the web server and blocks the main thread until it stops
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        println!("Server started on port {}", &self.port);
        self.server.await
    }
}

fn run(listener: TcpListener, settings: AppSettings) -> Result<Server, std::io::Error> {
    let port = listener
        .local_addr()
        .expect("TCPListener is invalid")
        .port();
    let settings = web::Data::new(settings);

    let server = HttpServer::new(move || {
        App::new()
            .route("/management/health", web::get().to(health_check))
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    println!("Sanitisium Web Server is running on port {port}");

    Ok(server)
}
