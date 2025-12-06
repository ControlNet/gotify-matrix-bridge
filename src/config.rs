use serde::Deserialize;
use std::{fs, path::Path, path::PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct RawConfig {
    pub gotify: GotifySection,
    pub matrix: MatrixSection,
    #[serde(default)]
    pub streams: Vec<StreamSection>,
    #[serde(default)]
    pub debug: bool,
    #[serde(rename = "templatePath", default = "default_template_path")]
    pub template_path: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GotifySection {
    pub url: String,
    #[serde(rename = "apiToken")]
    pub api_token: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MatrixSection {
    #[serde(rename = "homeserverURL")]
    pub homeserver_url: String,
    #[serde(rename = "matrixDomain", default)]
    pub matrix_domain: Option<String>,
    pub username: String,
    pub token: String,
    #[serde(rename = "roomID")]
    pub room_id: Option<String>,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(rename = "deviceID", default = "default_device_id")]
    pub device_id: String,
    #[serde(rename = "storePath", default = "default_store_path")]
    pub store_path: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StreamSection {
    #[serde(default)]
    #[allow(dead_code)]
    pub name: Option<String>,
    pub apps: Vec<i64>,
    pub rooms: Vec<String>,
    #[serde(rename = "template")]
    pub template_path: Option<PathBuf>,
}

fn default_template_path() -> PathBuf {
    PathBuf::from("messageTamplate.md")
}

fn default_device_id() -> String {
    "MGBRIDGE".to_string()
}

fn default_store_path() -> PathBuf {
    PathBuf::from(".matrix-gotify-bridge-store")
}

#[derive(Debug, Clone)]
pub struct Config {
    pub gotify_url: String,
    pub gotify_token: String,
    pub matrix_homeserver: String,
    pub matrix_domain: String,
    pub matrix_username: String,
    pub matrix_token: String,
    pub matrix_room: Option<String>,
    pub encrypted: bool,
    pub device_id: String,
    pub store_path: PathBuf,
    pub streams: Vec<StreamSection>,
    pub debug: bool,
    pub template_path: PathBuf,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Configuration error: {0}")]
    Invalid(String),
}

fn to_ws_scheme(url: &str) -> String {
    if url.starts_with("https://") {
        format!("wss://{}", &url[8..])
    } else if url.starts_with("http://") {
        format!("ws://{}", &url[7..])
    } else if url.starts_with("ws://") || url.starts_with("wss://") {
        url.to_string()
    } else {
        format!("wss://{}", url)
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let cfg_path = path.as_ref();
        let base_dir = cfg_path.parent().unwrap_or_else(|| Path::new("."));

        let buf = fs::read_to_string(cfg_path)?;
        let raw: RawConfig = serde_yaml::from_str(&buf)?;

        if raw.gotify.url.is_empty() {
            return Err(ConfigError::Invalid("gotify.url is required".into()));
        }
        if raw.gotify.api_token.is_empty() {
            return Err(ConfigError::Invalid("gotify.apiToken is required".into()));
        }
        if raw.matrix.homeserver_url.is_empty() {
            return Err(ConfigError::Invalid("matrix.homeserverURL is required".into()));
        }
        if raw.matrix.username.is_empty() {
            return Err(ConfigError::Invalid("matrix.username is required".into()));
        }
        if raw.matrix.token.is_empty() {
            return Err(ConfigError::Invalid("matrix.token is required".into()));
        }
        if raw.streams.is_empty() && raw.matrix.room_id.is_none() {
            return Err(ConfigError::Invalid(
                "Provide matrix.roomID or at least one stream".into(),
            ));
        }

        let domain = raw
            .matrix
            .matrix_domain
            .clone()
            .unwrap_or_else(|| raw.matrix.homeserver_url.replace("https://", "").replace("http://", ""));

        let resolved_template_path = if raw.template_path.is_absolute() {
            raw.template_path.clone()
        } else {
            base_dir.join(&raw.template_path)
        };

        let resolved_streams: Vec<StreamSection> = raw
            .streams
            .into_iter()
            .map(|mut s| {
                if let Some(t) = &s.template_path {
                    if !t.is_absolute() {
                        s.template_path = Some(base_dir.join(t));
                    }
                }
                s
            })
            .collect();

        Ok(Config {
            gotify_url: to_ws_scheme(&raw.gotify.url),
            gotify_token: raw.gotify.api_token,
            matrix_homeserver: raw.matrix.homeserver_url,
            matrix_domain: domain,
            matrix_username: raw.matrix.username,
            matrix_token: raw.matrix.token,
            matrix_room: raw.matrix.room_id,
            encrypted: raw.matrix.encrypted,
            device_id: raw.matrix.device_id,
            store_path: raw.matrix.store_path,
            streams: resolved_streams,
            debug: raw.debug,
            template_path: resolved_template_path,
        })
    }

    pub fn user_id(&self) -> String {
        format!("@{}:{}", self.matrix_username, self.matrix_domain)
    }
}
