FROM rust:1.84-alpine AS builder

RUN apk add --no-cache build-base musl-dev sccache
ENV RUSTC_WRAPPER=/usr/bin/sccache
ENV SCCACHE_GHA_ENABLED=true

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY example.config.yaml messageTamplate.default.md ./

RUN --mount=type=secret,id=ACTIONS_RESULTS_URL,env=ACTIONS_RESULTS_URL \
    --mount=type=secret,id=ACTIONS_RUNTIME_TOKEN,env=ACTIONS_RUNTIME_TOKEN \
    cargo build --release && \
    sccache --show-stats || echo "sccache stats unavailable"

FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata

COPY --from=builder /src/target/release/matrix-gotify-bridge /usr/local/bin/matrix-gotify-bridge
COPY --from=builder /src/example.config.yaml /usr/local/share/matrix-gotify-bridge/example.config.yaml
COPY --from=builder /src/messageTamplate.default.md /usr/local/share/matrix-gotify-bridge/messageTamplate.default.md

WORKDIR /data
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/matrix-gotify-bridge"]
CMD ["-c", "config.yaml"]
