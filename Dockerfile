# syntax=docker/dockerfile:1.7

FROM debian:bookworm-slim AS model-fetch
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fL -o /ggml-large-v3-turbo.bin \
    https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin


FROM rust:1-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        cmake \
        clang \
        libclang-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY short_generator ./short_generator

WORKDIR /build/short_generator
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/short_generator/target \
    cargo build --release \
 && cp target/release/short_generator /usr/local/bin/short_generator


FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ffmpeg \
        ca-certificates \
        fontconfig \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /app/models /app/downloads /app/short_generator

WORKDIR /app/short_generator

COPY --from=builder /usr/local/bin/short_generator /usr/local/bin/short_generator
COPY --from=model-fetch /ggml-large-v3-turbo.bin /app/models/ggml-large-v3-turbo.bin

# Bundled caption fonts (resolved via the subtitles filter's fontsdir=../fonts).
# Color emoji are fetched at runtime as Twemoji PNGs and composited by ffmpeg,
# since libass cannot render color-bitmap emoji fonts.
COPY fonts /app/fonts
RUN fc-cache -f /app/fonts

EXPOSE 3000

CMD ["short_generator"]