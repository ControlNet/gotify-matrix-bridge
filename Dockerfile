FROM rust:1.84-alpine AS builder

ENV RUSTC_WRAPPER=/usr/bin/sccache \
    SCCACHE_DIR=/sccache \
    SCCACHE_NO_DAEMON=1

RUN apk add --no-cache build-base musl-dev curl tar; \
    set -eux; \
    ARCH="$(uname -m)"; \
    case "$ARCH" in \
      x86_64)  SFX=x86_64-unknown-linux-musl ;; \
      aarch64) SFX=aarch64-unknown-linux-musl ;; \
      *) echo "unsupported arch: $ARCH" >&2; exit 1 ;; \
    esac; \
    curl -L "https://github.com/mozilla/sccache/releases/download/v0.12.0/sccache-v0.12.0-${SFX}.tar.gz" \
      -o /tmp/sccache.tar.gz; \
    mkdir -p /tmp/sccache && tar -xzf /tmp/sccache.tar.gz -C /tmp/sccache; \
    install /tmp/sccache/sccache-v0.12.0-${SFX}/sccache ${RUSTC_WRAPPER}; \
    rm -rf /tmp/sccache /tmp/sccache.tar.gz

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY example.config.yaml messageTamplate.default.md ./

RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=cargo-git \
    --mount=type=cache,target=/src/target,id=cargo-target \
    --mount=type=cache,target=/sccache,id=sccache,sharing=locked \
    set -eux; \
    cargo build --release; \
    sccache --show-stats || true

FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata

COPY --from=builder /src/target/release/matrix-gotify-bridge /usr/local/bin/matrix-gotify-bridge
COPY --from=builder /src/example.config.yaml /usr/local/share/matrix-gotify-bridge/example.config.yaml
COPY --from=builder /src/messageTamplate.default.md /usr/local/share/matrix-gotify-bridge/messageTamplate.default.md

WORKDIR /data
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/matrix-gotify-bridge"]
CMD ["-c", "config.yaml"]
