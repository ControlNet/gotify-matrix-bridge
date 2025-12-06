use anyhow::Context;
use futures::{StreamExt};
use log::{info, warn};
use serde::Deserialize;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::connect_async;

use crate::config::Config;

#[derive(Debug, Clone, Deserialize)]
pub struct GotifyEnvelope {
    #[serde(default)]
    pub message: Option<GotifyMessageRaw>,
    #[serde(flatten)]
    #[allow(dead_code)]
    pub other: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GotifyMessageRaw {
    pub appid: i64,
    pub id: i64,
    pub title: Option<String>,
    pub message: Option<String>,
    pub priority: Option<i64>,
    #[serde(default)]
    pub extras: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct GotifyMessage {
    pub app_id: i64,
    pub id: i64,
    pub title: Option<String>,
    pub message: Option<String>,
    pub priority: i64,
    #[allow(dead_code)]
    pub extras: serde_json::Value,
    #[allow(dead_code)]
    pub raw: serde_json::Value,
}

impl GotifyMessage {
    fn from_value(val: serde_json::Value) -> anyhow::Result<Self> {
        // handle envelope or bare message
        if let Ok(env) = serde_json::from_value::<GotifyEnvelope>(val.clone()) {
            if let Some(msg) = env.message {
                return GotifyMessage::from_raw(msg, val);
            }
        }
        let raw: GotifyMessageRaw = serde_json::from_value(val.clone())
            .context("parsing gotify message")?;
        GotifyMessage::from_raw(raw, val)
    }

    fn from_raw(raw: GotifyMessageRaw, val: serde_json::Value) -> anyhow::Result<Self> {
        Ok(Self {
            app_id: raw.appid,
            id: raw.id,
            title: raw.title,
            message: raw.message,
            priority: raw.priority.unwrap_or(0),
            extras: raw.extras,
            raw: val,
        })
    }
}

pub async fn listen<F>(cfg: &Config, mut handler: F) -> anyhow::Result<()>
where
    F: FnMut(GotifyMessage) -> futures::future::BoxFuture<'static, ()> + Send + 'static,
{
    let stream_url = format!("{}/stream?token={}", cfg.gotify_url.trim_end_matches('/'), cfg.gotify_token);
    let mut backoff = 2u64;
    loop {
        info!("Connecting to Gotify at {}", stream_url);
        match connect_async(&stream_url).await {
            Ok((ws_stream, _)) => {
                backoff = 2;
                info!("Connected to Gotify stream");
                let (_, mut read) = ws_stream.split();
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(ws_msg) => {
                            if ws_msg.is_text() {
                                let txt = ws_msg.into_text().unwrap_or_default();
                                match serde_json::from_str::<serde_json::Value>(&txt)
                                    .map_err(|e| anyhow::anyhow!(e))
                                    .and_then(GotifyMessage::from_value)
                                {
                                    Ok(parsed) => {
                                        handler(parsed).await;
                                    }
                                    Err(err) => warn!("Failed to parse gotify message: {}", err),
                                }
                            }
                        }
                        Err(err) => {
                            warn!("Websocket read error: {}", err);
                            break;
                        }
                    }
                }
            }
            Err(err) => {
                warn!("Failed to connect to Gotify: {}", err);
            }
        }
        warn!("Disconnected from Gotify, reconnecting in {}s", backoff);
        sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}
