# === Builder Stage ===
FROM docker.io/library/rust:bookworm AS builder

WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src/ src/

RUN cargo build --release --locked

# === Runtime Stage ===
FROM docker.io/library/debian:bookworm-slim

LABEL org.opencontainers.image.source=https://github.com/git001/mergelog-rs
LABEL org.opencontainers.image.description="Merge and sort HTTP log files chronologically – Rust rewrite of mergelog 4.5"
LABEL org.opencontainers.image.licenses="GPL-3.0-or-later"

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        bash \
        curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/mergelog-rs /usr/local/bin/mergelog-rs

ENTRYPOINT ["mergelog-rs"]
CMD ["--help"]

# Build:
#   podman build -t mergelog-rs -f Containerfile .
#
# Run:
#   podman run --rm -v /var/log:/logs:ro mergelog-rs /logs/access1.log /logs/access2.log
#   cat access.log.gz | podman run --rm -i mergelog-rs -
