FROM rust:alpine AS builder

RUN apk add --no-cache build-base pkgconfig openssl-dev

WORKDIR /workspace
COPY . .

RUN cargo build --manifest-path worker/Cargo.toml --release

# Source static ffmpeg/ffprobe binaries (multi-arch, no package manager required).
FROM mwader/static-ffmpeg:latest AS ffmpeg

FROM n8nio/n8n:latest

USER root

# Install static ffmpeg binaries in hardened n8n image.
COPY --from=ffmpeg /ffmpeg /usr/local/bin/ffmpeg
COPY --from=ffmpeg /ffprobe /usr/local/bin/ffprobe

# Install latest standalone yt-dlp binary for musl-based environments.
RUN set -eux; \
        arch="$(uname -m)"; \
        case "$arch" in \
            aarch64|arm64) ytdlp_asset="yt-dlp_musllinux_aarch64" ;; \
            x86_64|amd64) ytdlp_asset="yt-dlp_musllinux" ;; \
            *) echo "Unsupported architecture: $arch" >&2; exit 1 ;; \
        esac; \
        wget -O /usr/local/bin/yt-dlp "https://github.com/yt-dlp/yt-dlp/releases/latest/download/${ytdlp_asset}"; \
        chmod +x /usr/local/bin/yt-dlp; \
        ffmpeg -version | head -n 1; \
        ffprobe -version | head -n 1; \
        yt-dlp --version

# Install Node.js global dependencies
RUN npm install -g cheerio && npm cache clean --force

COPY --from=builder /workspace/worker/target/release/omni-downloader /usr/local/bin/omni-downloader

RUN mkdir -p /data/ingest /data/retention /home/node/.local/share/omni-downloader \
    && chown -R node:node /data /home/node/.local/share/omni-downloader

ENV NODE_PATH=/usr/local/lib/node_modules
ENV NODE_FUNCTION_ALLOW_EXTERNAL=cheerio

USER node

EXPOSE 5678