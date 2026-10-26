# shelf-server

An axum server (`crates/server`) that serves one library over HTTP and serves the built frontend. Meant to run in Docker on a home server.

## What it does

`main.rs`:
1. Installs JSON logging (`logging::init`).
2. Opens `BookLibrary` at **`/books`** (fixed) with a no-op `on_change`. This runs the full startup sync, so a big library delays the server's start. The watcher then keeps it current.
3. Routes: `/api/*` → `api::router()` with the request-ID and access-log layers; everything else → `frontend::serve`.
4. Listens on **`0.0.0.0:8080`** (fixed). Shuts down gracefully on Ctrl-C or SIGTERM (`docker stop`).

If opening the library or binding fails, it logs one `server failed` JSON line and exits with status 1.

There is no config file, no authentication, no TLS. Put it on a trusted network or behind a reverse proxy.

## Files

| File | Role |
|---|---|
| `main.rs` | Startup, router assembly, graceful shutdown |
| `api.rs` | One handler per `Library` method; `AppState = Arc<BookLibrary>`. Routes are listed in [api.md](api.md). |
| `error.rs` | `ApiErrorResponse`: `LibraryError` → status + `ApiError` JSON. The only place request errors are logged (404 at debug, 500 at error). |
| `frontend.rs` | Serves `frontend/dist` embedded with `rust-embed`. Unknown paths get `index.html` (client-side routes). `assets/*` (hashed names) get `Cache-Control: public, max-age=31536000, immutable`, the rest `no-cache`. Built without `dist`, it answers `404 the frontend isn't built`. |
| `logging.rs` | Subscriber setup and the access-log middleware. |

Compared to Go: the handlers are like `http.HandlerFunc`s whose decoding and encoding is done by axum extractors (`State`, `Json`, `Path`); `.layer(...)` is middleware wrapping, applied outermost-last.

## Logging

All logs go to stdout as one JSON object per line (`tracing-subscriber` with `.json().flatten_event(true).with_current_span(true).with_span_list(false)`). Level comes from `RUST_LOG`, default `info`.

- Every line has `timestamp`, `level`, `target`, `message` (`fields.*` flattened to the top level).
- Lines emitted while handling a request carry a `span` object with that request's `request_id`.
- **Access log**: one line per `/api` request with `target: "access"` and fields `request_id`, `method`, `path`, `status`, `latency_ms`, `req_bytes`, `resp_bytes`, `client_ip`, `user_agent`. Request IDs come from tower-http (`SetRequestIdLayer` + `PropagateRequestIdLayer`, header `x-request-id`, UUIDs).
- `client_ip` is the direct TCP peer; behind a proxy it's the proxy's address (`X-Forwarded-For` isn't read).
- Static frontend requests aren't access-logged.

```fish
docker logs shelf | jq 'select(.target == "access")'
```

## Docker image

`Dockerfile` (repo root), three stages:

1. **frontend**: `node:22.22.2-trixie-slim` + `buf` 1.75.0 + `pnpm` 12.15.0; `pnpm --dir frontend build` (which runs `buf generate` first).
2. **server**: `rust:1.99.1-slim-trixie`; copies the workspace (including `app/`, only so Cargo can resolve the workspace) and the built `frontend/dist`; `cargo build --release --locked -p shelf-server` with cache mounts for the registry and `target/`.
3. **runtime**: `debian:trixie-slim`, the binary only, runs as **uid/gid 1000**, `VOLUME /books`, `EXPOSE 8080`.

`.dockerignore` excludes `target`, `node_modules`, `frontend/dist`, `frontend/src/gen`, `app/gen` and `.git`.

The library folder mounted at `/books` must be writable by uid 1000, because the server writes `.shelf/` into it.

## Publishing (manual, no CI)

The image is published by hand to GitHub Container Registry as `ghcr.io/psomani16k/shelf`. Run from the repo root (fish-compatible):

```fish
# Once per machine: log in with a classic personal access token that has write:packages.
docker login ghcr.io -u psomani16k

# Each release: tag with the version and latest, then push both.
docker build -t ghcr.io/psomani16k/shelf:0.1.0 -t ghcr.io/psomani16k/shelf:latest .
docker push --all-tags ghcr.io/psomani16k/shelf
```

- A successful push ends with a `digest: sha256:…` line per tag. `docker manifest inspect ghcr.io/psomani16k/shelf:latest` confirms it's in the registry.
- Keep the tag in step with `version` in `crates/server/Cargo.toml` (logged at startup as `version`).
- A new ghcr package starts **private**. To make it public: github.com → your profile → *Packages* → `shelf` → *Package settings* → *Danger Zone* → *Change visibility* → *Public*. While private, the pulling machine needs `docker login ghcr.io` with a `read:packages` token.
- "manifest unknown" on pull means the tag isn't in the registry (the push didn't complete) or, for a private package, the puller isn't logged in.

As of 2026-10-26, version 0.1.0 had been built locally and a push attempted; that it landed and that the package is public hadn't been confirmed.

## Deploying with compose

`docker-compose.yml` (repo root) is a sample to copy next to the books folder on the server:

```yaml
services:
  shelf:
    image: ghcr.io/psomani16k/shelf:latest
    container_name: shelf
    restart: unless-stopped
    ports: ["8080:8080"]
    volumes: ["./books:/books"]       # must be writable by uid 1000
    environment: { RUST_LOG: info }
    logging:                          # json-file never rotates by default
      driver: json-file
      options: { max-size: "10m", max-file: "3" }
```

```fish
docker compose pull; and docker compose up -d
docker compose logs -f shelf
```

If `./books` doesn't exist, Docker creates it owned by root and the server can't write `.shelf/`. Create it first, or `sudo chown 1000:1000 books`.

## Running locally without Docker

The library path is hard-coded to `/books`, so locally that folder must exist and be readable and writable:

```fish
pnpm --dir frontend build    # optional: without it the server returns 404 for the UI
cargo run -p shelf-server
```

For frontend work, run the server and `pnpm --dir frontend dev`: Vite proxies `/api` to `http://localhost:8080`.
