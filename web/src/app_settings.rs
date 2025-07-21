use std::path::PathBuf;

use serde::Deserialize;
use serde_aux::field_attributes::deserialize_number_from_string;

#[derive(Clone, Deserialize)]
pub struct AppSettings {
    pub application: WebServerConfig,
}

#[derive(Clone, Deserialize)]
pub struct WebServerConfig {
    pub host: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
}

pub fn get_app_settings() -> Result<AppSettings, config::ConfigError> {
    let base_path =
        std::env::current_dir().expect("Failed to determine current dir while loading config");
    let config_dir = base_path.join("web").join("config");

    let env_config_file = std::env::var("ENV_CONFIG_FILE").ok();

    let builder =
        config::Config::builder().add_source(config::File::from(config_dir.join("base.yaml")));

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
