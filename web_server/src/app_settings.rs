use std::path::PathBuf;

use serde::Deserialize;
use serde_aux::field_attributes::deserialize_number_from_string;

#[derive(Clone, Deserialize)]
pub struct AppSettings {
    pub application: WebServerConfig,
    pub sanitisation: SanitisationConfig,
}

#[derive(Clone, Deserialize)]
pub struct SanitisationConfig {
    /// Base directory to store PDF files for processing sanitisation requests
    pub pdfs_dir: String,
}

#[derive(Clone, Deserialize)]
pub struct WebServerConfig {
    /// Host where to bind the web server
    pub host: String,
    /// Port to bind the web server to and listen for requests
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
}

pub fn get_app_settings() -> Result<AppSettings, config::ConfigError> {
    let mut base_path =
        std::env::current_dir().expect("Failed to determine current dir while loading config");

    // During integration tests, cargo invokes the binary from within the
    // crate directory, which changes the `current_dir`.
    // It's safe to pop out the crate directory from the path.
    let crate_name = env!("CARGO_CRATE_NAME");
    if base_path.ends_with(crate_name) {
        base_path.pop();
    }

    let config_dir = base_path.join("resources").join("config");

    // Optionally provide a way for the user to provide a custom YML config file
    // That could potentally be mounted onto the Docker container
    let env_config_file = std::env::var("ENV_CONFIG_FILE").ok();

    let builder =
        config::Config::builder().add_source(config::File::from(config_dir.join("base.yml")));

    let builder = match env_config_file {
        Some(file_path) => builder.add_source(config::File::from(PathBuf::from(file_path))),
        None => builder,
    };

    let builder = builder
        .add_source(
            // This allows us to overwrite any of our config values
            // using environment variables starting with `APP_`
            // For instance, to overwrite the port number, we can pass
            // in the following variable:
            // APP_APPLICATION__PORT=3000
            // APP_APPLICATION__HOST=0.0.0.0
            config::Environment::with_prefix("APP")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()?;

    builder.try_deserialize::<AppSettings>()
}
