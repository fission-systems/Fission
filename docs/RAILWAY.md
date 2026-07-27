# Railway deployment

Railway runs `fission-serve` as a persistent Docker service. The image contains
the version-matched SLEIGH, signature, FID, and type-information resources, so
the deployed service does not depend on a developer checkout or a volume.

`utils/` is intentionally excluded from Git. The Docker resource stage downloads
the version-pinned `fission-utils.tar.gz` release asset and verifies its SHA-256
before either the Rust build or runtime image can use it. When updating
`FISSION_UTILS_TAG` in `Dockerfile`, update `FISSION_UTILS_SHA256` from the same
release asset at the same time.

The runtime explicitly maps the packaged SLEIGH specifications and Ghidra
opinion data through `FISSION_SLEIGH_SPEC_DIR` and
`FISSION_GHIDRA_DATA_DIR`.

## Required service variables

```text
PORT=7331
```

`FISSION_SERVE_API_TOKEN` is no longer required in cloud mode. The reference
deployment's abuse guard is `fission-web`'s own nginx gateway
(`deploy/nginx/default.conf.template`), which rate-limits `/api/*` per client
IP (10 req/s, burst 20, `429` past that) before a request ever reaches the
backend. This removes the "generate a token, paste it into the browser"
friction for a public demo deployment; it is a deliberate tradeoff, not an
oversight -- an unauthenticated deployment is easier to abuse in aggregate
even with rate limiting, so keep the CPU/memory/usage limits from
[Current scaling boundary](#current-scaling-boundary) in place regardless.

If you do want bearer-token auth back (e.g. a private deployment, or
rate-limiting alone isn't enough for your traffic), set
`FISSION_SERVE_API_TOKEN` to a random value of at least 32 characters and the
existing `require_bearer` middleware re-engages automatically -- both
mechanisms can run at once (nginx still rate-limits in front either way):

```bash
openssl rand -hex 32
```

Do not put the token in any frontend build environment if you do set one:
values compiled into a WASM bundle are public. The web client only sends a
token if one has been entered in its own connection banner, and keeps it in
memory, not in the build.

Optional controls:

```text
FISSION_MAX_SESSIONS=10
FISSION_SESSION_TTL=1800
FISSION_MAX_UPLOAD_BYTES=52428800
RUST_LOG=fission_serve=info,tower_http=info
```

## Deploy

1. Create the private `fission-backend` Railway service from
   `fission-systems/Fission`.
2. Keep the repository root as the service root. Railway detects `Dockerfile`
   and `railway.json`.
3. Configure `PORT` (and, optionally, `FISSION_SERVE_API_TOKEN` -- see above)
   before the first deployment.
4. Create the public `fission-web` service from
   `fission-systems/fission-web`.
5. Keep both services in the same Railway project and environment.
6. Generate a public domain only for `fission-web`.
7. Open the web application -- it connects automatically; no token needed
   unless you configured one in step 3.

The public Nginx service serves the WASM application and proxies same-origin
`/api/*` requests to `fission-backend.railway.internal:7331`, rate-limiting
those requests per client IP on the way through. The backend does not need a
public domain.

## Current scaling boundary

Analysis sessions are currently held in process memory. Use exactly one Railway
replica; a restart or deployment expires active sessions. Horizontal scaling
requires moving uploaded binaries and session metadata to object storage and a
shared database or queue.

Set Railway replica CPU/memory limits and a workspace usage hard limit before
making the service public. Decompilation is CPU-intensive, and Railway does not
provide application-layer WAF protection.
