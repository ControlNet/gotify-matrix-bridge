mod config;
mod gotify;
mod matrix;
mod router;
mod template;

use crate::config::Config;
use crate::router::Router;
use crate::template::TemplateEngine;
use crate::gotify::GotifyMessage;
use anyhow::Context;
use futures::FutureExt;
use log::{info, LevelFilter};
use std::{fs, path::Path};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "matrix-gotify-bridge", about = "Bridge Gotify notifications into Matrix rooms.")]
struct Cli {
    /// Path to config.yaml
    #[structopt(short = "c", long = "config", default_value = "config.yaml")]
    config: String,

    /// Override log level (info, debug, warn, error)
    #[structopt(long = "log-level")]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::from_args();
    let created = ensure_default_files(&cli.config)?;
    if created {
        info!(
            "A default config was created at {}. Please edit it with your Gotify/Matrix settings and rerun.",
            cli.config
        );
        return Ok(());
    }

    let cfg = Config::load(&cli.config).context("loading config")?;
    ensure_template_files(&cfg)?;
    init_logging(&cfg, cli.log_level.as_deref());

    info!("Starting matrix-gotify-bridge");

    let template_engine = TemplateEngine::new(cfg.template_path.clone());
    let router = Router::new(cfg.streams.clone(), cfg.matrix_room.clone());
    let matrix = matrix::MatrixBridge::new(&cfg).await?;
    matrix.start_sync(cfg.encrypted).await?;
    // Pre-resolve and join all configured rooms (IDs or aliases) once at startup.
    let mut room_targets = std::collections::HashSet::new();
    if let Some(r) = &cfg.matrix_room {
        room_targets.insert(r.clone());
    }
    for stream in &cfg.streams {
        for r in &stream.rooms {
            room_targets.insert(r.clone());
        }
    }
    matrix.prepare_rooms(&room_targets.into_iter().collect::<Vec<_>>()).await;
    print_status(&cfg, &matrix).await?;

    let cfg_clone = cfg.clone();
    let handler = move |msg: GotifyMessage| {
        let template_engine = template_engine.clone();
        let router = router.clone();
        let matrix = matrix.clone();
        async move {
            let routes = router.routes_for(msg.app_id);
            if routes.is_empty() {
                log::info!("No routes for app {}, dropping message {}", msg.app_id, msg.id);
                return;
            }
            for route in routes {
                let rendered = template_engine.render(
                    &msg,
                    route.template.as_deref(),
                );
                if let Err(err) = matrix.send_markdown(&route.room_id, &rendered).await {
                    log::warn!("Failed to send to {}: {}", route.room_id, err);
                }
            }
        }
        .boxed()
    };

    gotify::listen(&cfg_clone, handler).await?;
    Ok(())
}

fn init_logging(cfg: &Config, override_level: Option<&str>) {
    let level = override_level
        .and_then(|lvl| lvl.parse::<LevelFilter>().ok())
        .unwrap_or_else(|| if cfg.debug { LevelFilter::Debug } else { LevelFilter::Info });

    env_logger::Builder::from_default_env()
        .filter_level(level)
        .init();
}

fn ensure_default_files(config_path: &str) -> anyhow::Result<bool> {
    let mut created = false;
    let cfg_path = Path::new(config_path);
    if !cfg_path.exists() {
        fs::write(cfg_path, include_str!("../example.config.yaml"))
            .with_context(|| format!("writing default config to {}", cfg_path.display()))?;
        created = true;
    }

    let tpl_path = Path::new("messageTamplate.md");
    if !tpl_path.exists() {
        fs::write(tpl_path, include_str!("../messageTamplate.default.md"))
            .with_context(|| format!("writing default template to {}", tpl_path.display()))?;
    }

    Ok(created)
}

fn ensure_template_files(cfg: &Config) -> anyhow::Result<()> {
    use std::collections::HashSet;
    let mut paths: HashSet<&Path> = HashSet::new();
    paths.insert(cfg.template_path.as_path());
    for stream in &cfg.streams {
        if let Some(p) = &stream.template_path {
            paths.insert(p.as_path());
        }
    }
    for path in paths {
        if !path.exists() {
            fs::write(path, include_str!("../messageTamplate.default.md"))
                .with_context(|| format!("writing default template to {}", path.display()))?;
        }
    }
    Ok(())
}

async fn print_status(cfg: &Config, matrix: &matrix::MatrixBridge) -> anyhow::Result<()> {
    // Gather entries: source, apps, room, status
    struct Row {
        source: String,
        apps: String,
        room: String,
        status: String,
    }
    let mut rows = Vec::new();

    if let Some(room) = &cfg.matrix_room {
        let ok = matrix.room_joined(room).await.unwrap_or(false);
        rows.push(Row {
            source: "default".into(),
            apps: "-".into(),
            room: room.clone(),
            status: if ok { "joined" } else { "not joined" }.into(),
        });
    }

    for (idx, stream) in cfg.streams.iter().enumerate() {
        let source = stream
            .name
            .clone()
            .unwrap_or_else(|| format!("stream{}", idx + 1));
        let apps = stream
            .apps
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(",");
        for room in &stream.rooms {
            let ok = matrix.room_joined(room).await.unwrap_or(false);
            rows.push(Row {
                source: source.clone(),
                apps: apps.clone(),
                room: room.clone(),
                status: if ok { "joined" } else { "not joined" }.into(),
            });
        }
    }

    // Compute column widths
    let mut w_source = "Source".len();
    let mut w_apps = "Apps".len();
    let mut w_room = "Room".len();
    let mut w_status = "Status".len();
    for r in &rows {
        w_source = w_source.max(r.source.len());
        w_apps = w_apps.max(r.apps.len());
        w_room = w_room.max(r.room.len());
        w_status = w_status.max(r.status.len());
    }
    let sep = format!(
        "+-{}-+-{}-+-{}-+-{}-+",
        "-".repeat(w_source),
        "-".repeat(w_apps),
        "-".repeat(w_room),
        "-".repeat(w_status)
    );
    println!("{}", sep);
    println!(
        "| {source:<s_w$} | {apps:<a_w$} | {room:<r_w$} | {status:<t_w$} |",
        source = "Source",
        apps = "Apps",
        room = "Room",
        status = "Status",
        s_w = w_source,
        a_w = w_apps,
        r_w = w_room,
        t_w = w_status
    );
    println!("{}", sep);
    for r in rows {
        println!(
            "| {source:<s_w$} | {apps:<a_w$} | {room:<r_w$} | {status:<t_w$} |",
            source = r.source,
            apps = r.apps,
            room = r.room,
            status = r.status,
            s_w = w_source,
            a_w = w_apps,
            r_w = w_room,
            t_w = w_status
        );
    }
    println!("{}", sep);
    Ok(())
}
