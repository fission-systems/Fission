# ── Stage 0: Versioned runtime resources ─────────────────────────────────────
FROM debian:bookworm-slim AS resources

ARG FISSION_UTILS_TAG=v0.1.6
ARG FISSION_UTILS_SHA256=35dd98b3cb4e4a6b1baac410b64ab1e62a78e4d0c5464332f90840b4adc095e7

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN curl --fail --location --show-error \
    "https://github.com/fission-systems/Fission/releases/download/${FISSION_UTILS_TAG}/fission-utils.tar.gz" \
    --output /tmp/fission-utils.tar.gz \
    && echo "${FISSION_UTILS_SHA256}  /tmp/fission-utils.tar.gz" | sha256sum --check --strict \
    && mkdir -p /bundle \
    && tar -xzf /tmp/fission-utils.tar.gz -C /bundle \
    && test -f /bundle/utils/sleigh-specs/ghidra_language_manifest.json \
    && rm /tmp/fission-utils.tar.gz

# ── Stage 1: Build ────────────────────────────────────────────────────────────
FROM rust:1.97-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependencies — copy manifests first
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY --from=resources /bundle/utils/sleigh-specs/ ./utils/sleigh-specs/

ENV FISSION_SLEIGH_SPEC_DIR=/build/utils/sleigh-specs

# Build only fission-serve in release mode
RUN cargo build -p fission-serve --release

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary
COPY --from=builder /build/target/release/fission-serve ./fission-serve

# Copy fission-utils (sleigh specs, signatures, type info)
# These are required by the SLEIGH runtime for decompilation.
COPY --from=resources /bundle/utils/ ./utils/

# ── Runtime configuration (override via env vars or --flag) ───────────────────
ENV FISSION_SLEIGH_SPEC_DIR=/app/utils/sleigh-specs
ENV FISSION_SLEIGH_CACHE_DIR=/tmp/fission-sleigh
ENV FISSION_GHIDRA_DATA_DIR=/app/utils/ghidra-data
ENV FISSION_RESOURCE_ROOT=/app/utils
ENV FISSION_ROOT=/app
ENV FISSION_SERVE_HOST=0.0.0.0
ENV FISSION_CLOUD_MODE=true
ENV PORT=7331

# Railway / Fly.io inject $PORT at runtime — pass it through
EXPOSE 7331

ENTRYPOINT ["./fission-serve"]
CMD ["--host", "0.0.0.0"]
