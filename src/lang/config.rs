use std::path::Path;

use serde_derive::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigurationError {
    #[error("Error during Configuration IO")]
    IOError(#[from] std::io::Error),
    #[error("Malformed Configuration Data")]
    DeserializeError(#[from] toml::de::Error),
    #[error("Malformed Configuration Data during Write")]
    SerializeError(#[from] toml::ser::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    #[serde(default = "default_size")]
    pub window_size: (u32, u32),
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_size() -> (u32, u32) {
    (1920, 1080)
}

fn default_language() -> String {
    "en-US".to_string()
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            window_size: default_size(),
            language: default_language(),
        }
    }
}

impl Configuration {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigurationError> {
        let file = std::fs::read_to_string(path)?;
        Ok(toml::de::from_str(&file)?)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigurationError> {
        let data = toml::ser::to_string(self)?;
        std::fs::write(path, data).map_err(|e| e.into())
    }
}
