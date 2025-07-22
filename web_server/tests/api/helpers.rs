use std::sync::LazyLock;

use dotenv::dotenv;
use web_server::app_settings::get_app_settings;
use web_server::startup::Application;
use web_server::telemetry::{get_telemetry_subscriber, init_telemetry_subscriber};

pub struct TestApp {
    /// Address where our app will be listening to HTTP requests.
    /// Commonly using 127.0.0.1:[port] during local tests.
    /// This is helpful for issuing requests against
    /// our own app instance.
    ///
    /// Port is assigned dynamically based on what the OS provides.
    pub address: String,
}

#[allow(clippy::let_underscore_future)]
pub async fn spawn_app() -> TestApp {
    dotenv().ok();

    LazyLock::force(&TRACING);

    let mut settings = get_app_settings().expect("Could not get App Settings");
    // using "0" as port will let the OS bind our test server to
    // a random available port. This allows us to run multiple instances
    // of our web server and test it in parallel
    settings.application.port = 0;

    let app: Application = Application::build(settings)
        .await
        .expect("Could not build Application server");

    let app_port = app.port();
    let _ = tokio::spawn(app.run_until_stopped());

    let address = format!("http://127.0.0.1:{app_port}");
    TestApp { address }
}

static TRACING: LazyLock<()> = LazyLock::new(|| {
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber =
            get_telemetry_subscriber("sanitisium", "alpha", "dev", "info", std::io::stdout);
        init_telemetry_subscriber(subscriber);
    } else {
        let subscriber =
            get_telemetry_subscriber("sanitisium", "alpha", "dev", "info", std::io::sink);
        init_telemetry_subscriber(subscriber);
    };
});
