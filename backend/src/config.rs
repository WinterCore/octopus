use std::{collections::HashSet, fs, path::{Path, PathBuf}};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub id: String,
    pub name: String,
    pub playlist: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_stream: Option<String>,
    #[serde(rename = "stream")]
    pub streams: Vec<StreamConfig>,
}

impl StreamsConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path).map_err(|e| format!("read config: {}", e))?;
        let config: StreamsConfig = toml::from_str(&raw).map_err(|e| format!("parse config: {}", e))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        for s in &self.streams {
            if s.id.is_empty() {
                return Err("stream id cannot be empty".to_string());
            }
            if !seen.insert(s.id.clone()) {
                return Err(format!("duplicate stream id: {}", s.id));
            }
        }
        if let Some(default) = &self.default_stream {
            if !self.streams.iter().any(|s| &s.id == default) {
                return Err(format!(
                    "default_stream '{}' does not match any defined stream",
                    default
                ));
            }
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let serialized = toml::to_string_pretty(self).map_err(|e| format!("serialize config: {}", e))?;
        let tmp: PathBuf = path.with_extension("toml.tmp");
        fs::write(&tmp, serialized).map_err(|e| format!("write tmp config: {}", e))?;
        fs::rename(&tmp, path).map_err(|e| format!("rename tmp config: {}", e))?;
        Ok(())
    }
}
