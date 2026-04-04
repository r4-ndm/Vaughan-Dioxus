# Optional CI/dev image: run the same test command as GitHub Actions.
# Full desktop builds still need a normal host with a display for GUI binaries.
FROM rust:1-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev patchelf cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo test --workspace --no-fail-fast
