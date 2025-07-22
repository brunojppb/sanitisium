use anyhow::Result;
use web_server::{
    app_settings::get_app_settings,
    startup::Application,
    telemetry::{get_telemetry_subscriber, init_telemetry_subscriber},
};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    // Init telemetry subscriber to process tracing spans and logs
    // TODO: Read these values from environment variables instead.
    // Leverage Otel default environment variables as much as possible later:
    // See: https://opentelemetry.io/docs/specs/otel/configuration/sdk-environment-variables/#general-sdk-configuration
    let subscriber =
        get_telemetry_subscriber("sanitisium", "alpha", "dev", "info", std::io::stdout);
    init_telemetry_subscriber(subscriber);

    let app_settings = get_app_settings().expect("Could not create AppSettings");
    let app = Application::build(app_settings).await?;
    app.run_until_stopped().await?;

    Ok(())
}
