# syntax=docker/dockerfile:1.7

FROM rust:1.92-slim-bookworm AS builder
WORKDIR /workspace

RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential protobuf-compiler pkg-config ca-certificates \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY proto ./proto
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/target \
    cargo build --locked --release --bin sqlrite \
    && cp /workspace/target/release/sqlrite /tmp/sqlrite

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --home-dir /var/lib/sqlrite --shell /usr/sbin/nologin sqlrite \
  && mkdir -p /data \
  && chown -R sqlrite:sqlrite /data /var/lib/sqlrite

COPY --from=builder /tmp/sqlrite /usr/local/bin/sqlrite

EXPOSE 8099
VOLUME ["/data"]

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8099/readyz >/dev/null || exit 1

USER sqlrite
ENTRYPOINT ["sqlrite"]
CMD ["serve", "--db", "/data/sqlrite.db", "--bind", "0.0.0.0:8099"]
