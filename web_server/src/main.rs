use web_server::{app_settings::get_app_settings, startup::Application};

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    dotenv::dotenv().ok();

    let app_settings = get_app_settings().expect("Could not create AppSettings");
    let app = Application::build(app_settings).await?;
    app.run_until_stopped().await?;

    Ok(())
}
