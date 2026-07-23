# ── Stage 1: Build ────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependencies — copy manifests first
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

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
COPY utils/ ./utils/

# ── Runtime configuration (override via env vars or --flag) ───────────────────
ENV FISSION_SLEIGH_SPEC_DIR=/app/utils/sleigh-specs
ENV FISSION_SERVE_HOST=0.0.0.0
ENV PORT=7331

# Railway / Fly.io inject $PORT at runtime — pass it through
EXPOSE 7331

ENTRYPOINT ["./fission-serve"]
CMD ["--host", "0.0.0.0"]
