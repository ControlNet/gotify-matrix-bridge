# Matrix-Gotify Bridge (Rust)

Bridge bot that forwards Gotify notifications to Matrix rooms. Compared with [Ondolin/gotify-matrix-bot](https://github.com/Ondolin/gotify-matrix-bot),  supports multi-stream routing (map app IDs to one or many rooms), optional per-stream templates, and optional end-to-end encryption.

## Quick start
### Docker

```bash
docker run -d -v <data-path>:/data --name matrix-gotify-bridge controlnet/matrix-gotify-bridge   # first run writes /data/config.yaml then exits
```

You may need `docker restart matrix-gotify-bridge` to apply changes of the config file.

### Native

```bash
cargo build --release
cp example.config.yaml config.yaml   # fill Gotify URL/token, Matrix creds, rooms, streams
cargo run --release -- -c config.yaml
```

## Config

See [example.config.yaml](/example.config.yaml) for a full sample.

Notes: Currently still not in production.
