FROM rust:1.84-alpine AS builder

RUN apk add --no-cache build-base musl-dev

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY example.config.yaml messageTamplate.default.md ./

RUN cargo build --release

FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata

COPY --from=builder /src/target/release/gotify-matrix-bridge /usr/local/bin/gotify-matrix-bridge
COPY --from=builder /src/example.config.yaml /usr/local/share/gotify-matrix-bridge/example.config.yaml
COPY --from=builder /src/messageTamplate.default.md /usr/local/share/gotify-matrix-bridge/messageTamplate.default.md

WORKDIR /data
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/gotify-matrix-bridge"]
CMD ["-c", "config.yaml"]
