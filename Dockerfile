FROM rust:1.86-bookworm AS builder
WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --bin sqlrite

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /workspace/target/release/sqlrite /usr/local/bin/sqlrite

EXPOSE 8099
VOLUME ["/data"]

ENTRYPOINT ["sqlrite"]
CMD ["serve", "--db", "/data/sqlrite.db", "--bind", "0.0.0.0:8099"]
