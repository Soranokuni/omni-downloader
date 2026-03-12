FROM rust:alpine AS builder

RUN apk add --no-cache build-base pkgconfig openssl-dev

WORKDIR /workspace
COPY . .

RUN cargo build --manifest-path worker/Cargo.toml --release

FROM n8nio/n8n:latest

USER root

RUN apk add --no-cache ffmpeg python3 curl ca-certificates

RUN curl -L "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp_linux" -o /usr/local/bin/yt-dlp \
    && chmod +x /usr/local/bin/yt-dlp

RUN npm install -g cheerio

COPY --from=builder /workspace/worker/target/release/omni-downloader /usr/local/bin/omni-downloader

RUN mkdir -p /data/ingest /data/retention /home/node/.local/share/omni-downloader \
    && chown -R node:node /data /home/node/.local/share/omni-downloader

ENV NODE_PATH=/usr/local/lib/node_modules
ENV NODE_FUNCTION_ALLOW_EXTERNAL=cheerio

USER node

EXPOSE 5678