# Multi-stage build for the AmberHTML CLI (Plans.md 6.7).
#
# STATUS (6.7, WIP scaffolding): not built/validated in this environment (no
# Docker here). Build + run with:
#   docker build -t amber-html .
#   docker run --rm -v "$PWD/out:/out" amber-html https://example.com --markdown -o /out
#
# A pinned Chrome for Testing is downloaded and cached on the first capture that
# needs a browser; mount a volume or set AMBER_CHROMIUM_PATH to reuse it.
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p amber-cli

FROM debian:stable-slim
# Runtime libraries Chromium needs once it is downloaded on first use.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libnss3 libatk1.0-0 libatk-bridge2.0-0 libcups2 \
        libdrm2 libxkbcommon0 libxcomposite1 libxdamage1 libxrandr2 \
        libgbm1 libpango-1.0-0 libcairo2 libasound2 fonts-liberation \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/amber /usr/local/bin/amber
# Cache the pinned browser under a stable, mountable path.
ENV AMBER_CACHE_DIR=/var/cache/amber
ENTRYPOINT ["amber"]
