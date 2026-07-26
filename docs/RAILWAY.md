# Railway deployment

Railway runs `fission-serve` as a persistent Docker service. The image contains
the version-matched SLEIGH, signature, FID, and type-information resources, so
the deployed service does not depend on a developer checkout or a volume.

`utils/` is intentionally excluded from Git. The Docker resource stage downloads
the version-pinned `fission-utils.tar.gz` release asset and verifies its SHA-256
before either the Rust build or runtime image can use it. When updating
`FISSION_UTILS_TAG` in `Dockerfile`, update `FISSION_UTILS_SHA256` from the same
release asset at the same time.

## Required service variables

```text
FISSION_SERVE_API_TOKEN=<random value of at least 32 characters>
FISSION_ALLOWED_ORIGINS=https://<fission-web-domain>
```

Generate the token locally and store it only in Railway and the analyst's
browser session:

```bash
openssl rand -hex 32
```

Do not put the token in the Vercel build environment: values compiled into a
WASM bundle are public. The web client prompts for the token and keeps it only
in memory.

Optional controls:

```text
FISSION_MAX_SESSIONS=10
FISSION_SESSION_TTL=1800
FISSION_MAX_UPLOAD_BYTES=52428800
RUST_LOG=fission_serve=info,tower_http=info
```

## Deploy

1. Create a Railway service from `fission-systems/Fission`.
2. Keep the repository root as the service root. Railway detects `Dockerfile`
   and `railway.json`.
3. Configure the required variables before the first deployment.
4. Generate a public Railway domain.
5. Build `fission-web` with `FISSION_WEB_API_URL` set to that HTTPS domain.
6. Open the web application and enter the bearer token when connecting.

`/healthz` is intentionally public for Railway deployment healthchecks. All
`/api/*` routes require the configured bearer token in cloud mode.

## Current scaling boundary

Analysis sessions are currently held in process memory. Use exactly one Railway
replica; a restart or deployment expires active sessions. Horizontal scaling
requires moving uploaded binaries and session metadata to object storage and a
shared database or queue.

Set Railway replica CPU/memory limits and a workspace usage hard limit before
making the service public. Decompilation is CPU-intensive, and Railway does not
provide application-layer WAF protection.
