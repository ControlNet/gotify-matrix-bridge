use anyhow::Context;
use log::{debug, info};
use matrix_sdk::{
    config::{RequestConfig, SyncSettings},
    matrix_auth::MatrixSession,
    ruma::{
        api::client::filter::FilterDefinition,
        events::room::message::RoomMessageEventContent,
        events::AnyMessageLikeEventContent,
        OwnedDeviceId, OwnedRoomAliasId, OwnedRoomId, OwnedUserId,
    },
    Client, SessionMeta, RoomState,
};
use pulldown_cmark::{html, Parser};
use std::fs;

use crate::config::Config;

#[derive(Clone)]
pub struct MatrixBridge {
    client: Client,
    alias_cache: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, OwnedRoomId>>>,
}

impl MatrixBridge {
    pub async fn new(cfg: &Config) -> anyhow::Result<Self> {
        fs::create_dir_all(&cfg.store_path)
            .with_context(|| format!("creating store path {}", cfg.store_path.display()))?;

        let client = Client::builder()
            .homeserver_url(&cfg.matrix_homeserver)
            .sqlite_store(&cfg.store_path, None)
            .request_config(RequestConfig::new().retry_timeout(std::time::Duration::from_secs(10)))
            .build()
            .await
            .context("build matrix client")?;

        let user_id: OwnedUserId = cfg.user_id().parse().context("parse user id")?;
        let device_id: OwnedDeviceId = cfg.device_id.clone().into();
        let session = MatrixSession {
            meta: SessionMeta { user_id: user_id.clone(), device_id },
            tokens: matrix_sdk::matrix_auth::MatrixSessionTokens { access_token: cfg.matrix_token.clone(), refresh_token: None },
        };
        client.restore_session(session).await.context("restore session")?;

        Ok(Self { client, alias_cache: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())) })
    }

    pub async fn start_sync(&self, encrypted: bool) -> anyhow::Result<()> {
        if encrypted {
            info!("Starting background sync for encryption support");
            let client = self.client.clone();
            tokio::spawn(async move {
                let filter = FilterDefinition::default().into();
                let settings = SyncSettings::default().filter(filter);
                if let Err(err) = client.sync(settings).await {
                    log::warn!("Sync loop ended: {}", err);
                }
            });
        } else {
            let settings = SyncSettings::default().timeout(std::time::Duration::from_millis(10_000));
            self.client.sync_once(settings).await?;
        }
        Ok(())
    }

    pub async fn prepare_rooms(&self, rooms: &[String]) {
        let mut uniq = std::collections::HashSet::new();
        for r in rooms {
            if !uniq.insert(r.clone()) {
                continue;
            }
            match self.resolve_and_join(r).await {
                Ok(_) => {
                    let joined = self.room_joined(r).await.unwrap_or(false);
                    if !joined {
                        log::warn!("Room {} still not joined after attempt (waiting for invite/permissions?)", r);
                    }
                }
                Err(err) => {
                    log::warn!("Room {} unavailable: {}", r, err);
                }
            }
        }
    }

    pub async fn send_markdown(&self, room_ref: &str, text: &str) -> anyhow::Result<()> {
        let room = self
            .get_joined_room(room_ref)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Not joined to room {}", room_ref))?;

        let html = markdown_to_html(text);
        let content = RoomMessageEventContent::text_html(text, html);
        let any = AnyMessageLikeEventContent::RoomMessage(content);
        room.send(any).await?;
        debug!("Sent message to {}", room_ref);
        Ok(())
    }

    pub async fn room_joined(&self, room_ref: &str) -> anyhow::Result<bool> {
        Ok(self.get_joined_room(room_ref).await?.is_some())
    }

    async fn resolve_and_join(&self, room_input: &str) -> anyhow::Result<()> {
        // If it's a room ID, try directly and join if needed
        if room_input.starts_with('!') {
            let room_id: OwnedRoomId = room_input.parse().context("parse room id")?;
            if let Some(room) = self.client.get_room(room_id.as_ref()) {
                if room.state() == RoomState::Joined {
                    return Ok(());
                }
            }
            // Best-effort join (covers the case where we've been invited)
            if let Err(err) = self.client.join_room_by_id(room_id.as_ref()).await {
                log::error!("Join failed for room {}: {}", room_input, err);
                return Err(err.into());
            }
            return Ok(());
        }

        // If it's an alias, resolve to room ID (cached) then join by ID to keep behavior consistent.
        if room_input.starts_with('#') {
            let alias: OwnedRoomAliasId = room_input.parse().context("parse room alias")?;

            if let Some(cached) = self.alias_cache.read().await.get(alias.as_str()).cloned() {
                if self
                    .client
                    .get_room(cached.as_ref())
                    .map(|r| r.state() == RoomState::Joined)
                    .unwrap_or(false)
                {
                    return Ok(());
                }
            }

            // Resolve alias to room id and candidate servers, then join by id using those servers.
            let resolution = self.client.resolve_room_alias(alias.as_ref()).await?;
            let room_id = resolution.room_id;
            let servers: Vec<matrix_sdk::ruma::OwnedServerName> = resolution.servers.into_iter().collect();

            // Try to join using room id; resolution ensures behaviour matches direct room-id joins.
            // Note: join_room_by_id currently ignores server hints, but the servers list is kept
            // for future use if the SDK exposes it.
            let _ = servers; // suppress unused in case the SDK signature changes
            if let Err(err) = self.client.join_room_by_id(room_id.as_ref()).await {
                log::error!("Join failed for alias {} -> {}: {}", alias, room_id, err);
                return Err(err.into());
            }

            self.alias_cache
                .write()
                .await
                .insert(alias.to_string(), room_id);
            return Ok(());
        }

        // Unknown format
        Err(anyhow::anyhow!("Room must be a room ID (!...) or alias (#...)"))
    }

    async fn get_joined_room(
        &self,
        room_input: &str,
    ) -> anyhow::Result<Option<matrix_sdk::room::Room>> {
        if room_input.starts_with('!') {
            let room_id: OwnedRoomId = room_input.parse().context("parse room id")?;
            let room = self.client.get_room(room_id.as_ref());
            return Ok(room.filter(|r| r.state() == RoomState::Joined));
        }

        if room_input.starts_with('#') {
            let alias: OwnedRoomAliasId = room_input.parse().context("parse room alias")?;
            if let Some(cached) = self.alias_cache.read().await.get(alias.as_str()).cloned() {
                let room = self.client.get_room(cached.as_ref());
                return Ok(room.filter(|r| r.state() == RoomState::Joined));
            }
            return Ok(None);
        }

        Err(anyhow::anyhow!("Room must be a room ID (!...) or alias (#...)"))
    }
}

fn markdown_to_html(md: &str) -> String {
    let parser = Parser::new(md);
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    html_out
}
