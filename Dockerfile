# Multi-stage image for the AmberHTML CLI (Plans.md 6.7).
#
#   docker build -t amber-html .
#   docker run --rm -v "$PWD/out:/out" amber-html https://example.com --markdown -o /out
#
# A pinned Chrome for Testing downloads on the first capture that needs a
# browser and is cached under AMBER_CACHE_DIR; mount it as a volume to persist
# across runs, or set AMBER_CHROMIUM_PATH to an existing Chromium.
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p amber-cli

FROM debian:stable-slim
LABEL org.opencontainers.image.title="AmberHTML" \
      org.opencontainers.image.description="Local-first web-page capture engine (CLI)." \
      org.opencontainers.image.source="https://github.com/afeique/amber-html" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0"

# Runtime libraries Chromium needs once it is downloaded on first use.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libnss3 libatk1.0-0 libatk-bridge2.0-0 libcups2 \
        libdrm2 libxkbcommon0 libxcomposite1 libxdamage1 libxrandr2 \
        libgbm1 libpango-1.0-0 libcairo2 libasound2 fonts-liberation \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/amber /usr/local/bin/amber

# Cache the pinned browser under a stable, mountable path; default outputs there.
ENV AMBER_CACHE_DIR=/var/cache/amber
WORKDIR /out
ENTRYPOINT ["amber"]
