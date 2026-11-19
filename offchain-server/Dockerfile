FROM rust:1.88-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/offchain-server /usr/local/bin/offchain-server
EXPOSE 8080
USER 65532:65532
ENTRYPOINT ["offchain-server"]
