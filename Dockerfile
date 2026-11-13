FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/kurye-takip-railway /usr/local/bin/kurye-takip-railway
CMD ["kurye-takip-railway"]
