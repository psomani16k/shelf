# shelf-server

An axum server (`crates/server`) that serves one library over HTTP and serves the built frontend. Meant to run in Docker on a home server.

## What it does

`main.rs`:
1. Installs JSON logging (`logging::init`).
2. Loads the server data from **`/data/data.json`** (fixed; see [Server data](#server-data)), creating it if it's missing, into `Auth`, and logs how many users it has.
3. Opens `ServerLibrary` (a `BookLibrary` plus a lock so only one rebuild runs at a time) at **`/books`** (fixed) with a no-op `on_change`. This runs the startup sync, which hashes only new and modified books (all of them on the very first start). The watcher then keeps it current.
4. Routes: `/api/*` → `api::router(state)` (`AppState { library, auth }`) with the request-ID and access-log layers; every API route but `health` and `auth/login` goes through `auth::authenticate` ([api.md](api.md#http-routes-cratesserversrcapirs)); everything else → `frontend::serve`. Request bodies are capped at axum's default 2 MB, except `/api/books/upload` at 512 MiB (`MAX_UPLOAD_BYTES` in `api.rs`); behind a reverse proxy, raise its limit too (Caddy has none by default, nginx's `client_max_body_size` is 1 MB).
5. Listens on **`0.0.0.0:8080`** (fixed). Shuts down gracefully on Ctrl-C or SIGTERM (`docker stop`).

If loading the server data, opening the library or binding fails, it logs one `server failed` JSON line and exits with status 1.

There is no config file and no TLS: put it behind a reverse proxy that terminates HTTPS (tokens and passwords are sent in every request). Every API route needs a login.

## Files

| File | Role |
|---|---|
| `main.rs` | Startup, router assembly, graceful shutdown |
| `api.rs` | One handler per library operation, each going through the user's `LibraryView` for anything that touches books, and the router: routes grouped by the lowest role that may call them. `AppState { library, auth }`, with `FromRef` so each handler takes only its part. Routes are listed in [api.md](api.md). |
| `auth.rs` | `Auth`: login, sessions, the lockout, password change. `auth/requests.rs`: sign-ups and approving them; `auth/users.rs`: listing users and changing roles, with `may_manage`, the one rule for who may give or take which role. The `authenticate` middleware (bearer token → `CurrentUser` in the request's extensions, like a value in a Go `context`), the `CurrentUser` extractor, `require(role)`, and the `/auth/*` handlers. |
| `data.rs` | `DataFile`: `/data/data.json` in memory, with each user's `Scope` built whenever it's read. `load` (creates it if missing), `refresh` (reloads after a hand edit), `update` (re-reads, changes, writes atomically). |
| `library.rs` | `ServerLibrary`: wraps the one `BookLibrary`. `view(&scope)` gives a user's part of it; config and `rebuild_index` (the whole library, inside `rebuild_lock`) go to it directly. |
| `error.rs` | `ApiErrorResponse`: `LibraryError` → status + `ApiError` JSON. The only place request errors are logged (4xx at debug, 500 at error). |
| `frontend.rs` | Serves `frontend/dist` embedded with `rust-embed`. Unknown paths get `index.html` (client-side routes). `assets/*` (hashed names) get `Cache-Control: public, max-age=31536000, immutable`, the rest `no-cache`. Built without `dist`, it answers `404 the frontend isn't built`. |
| `logging.rs` | Subscriber setup and the access-log middleware. |

Compared to Go: the handlers are like `http.HandlerFunc`s whose decoding and encoding is done by axum extractors (`State`, `Json`, `Path`); `.layer(...)` is middleware wrapping, applied outermost-last.

## Server data

`/data/data.json` (`serverpb.ServerData`, `proto/serverpb/data.proto`) is the server's own data, kept out of the library so it never syncs with it. All of it is in this one file: the users, keyed by user id, each with a password **in plain text** (a deliberate choice, see [decisions.md](decisions.md)), a role, the part of the library they see, and their signed-in sessions, plus the sign-ups waiting for an admin (`user_requests`). Users come from approved sign-ups or are added by hand; the server adds and removes sessions and requests, changes passwords and roles, and adds approved users:

```json
{
  "users": {
    "kid": {
      "password": "pancakes",
      "role": "READER",
      "library_root": "kid",
      "books": ["**/*.epub", "!scary/**"]
    },
    "parth": {
      "password": "hunter2",
      "role": "OWNER",
      "sessions": {
        "9f2c…": { "created": "2026-11-13T10:00:00Z", "expires": "2027-02-11T10:00:00Z" }
      }
    }
  },
  "user_requests": [
    { "user_name": "newbie", "password": "waffles", "id": "q3J…", "created": "2026-11-13T11:00:00Z" }
  ]
}
```

A session's key is the hex SHA-256 of its token, so the file never holds a token that works. To sign someone out everywhere, delete their `sessions`.

Roles (`apipb.Role.Value`, in `proto/apipb/auth.proto` because `/api/auth/me` returns it) are a ladder: each can do everything the ones before it can, so a check is `role >= needed`. Their numbers are that order, so they must never be renumbered.

| Role | Can |
|---|---|
| `BANNED` (0, the value when `role` is left out) | Nothing: signing in is a 403 saying the account is banned, and a session ends on its next request |
| `READER` | Read books and keep their own progress |
| `EDITOR` | Also add and delete books |
| `ADMIN` | Also rebuild the index, approve sign-ups as readers or editors, and move other users between `READER`, `EDITOR` and `BANNED`; later remove them and change which books each user can read (planned) |
| `OWNER` | Also make users admins (from a sign-up or an existing user) and change or ban admins; later remove them (planned). Only given or taken by editing `data.json` on the server, never through the UI. There can be any number of owners, including none |

No one can change their own role. Banning ends all of the user's sessions; other role changes keep them and apply on the next request. Which routes each role can call: [api.md](api.md#access-by-role).

**What a user sees** (`library_root` and `books`, set in *Settings → Admin* (*Access* on a user, stored with the root normalized and blank patterns dropped) or by editing the file; `Scope` in `crates/core/src/library/scope.rs`):

- **`library_root`** is a folder under `/books` that is the user's whole library; left out, it's all of `/books`. It follows the same rules as a folder path from a client (no `..`, no dot names). The user's paths are relative to it, so it shows as their library root and nothing they're sent names it.
- **`books`** is a list of globs for the books they see, relative to the root and matched against book paths, ignoring case. `*` stays within one folder and `**` crosses folders, so `**/*.epub` is every book and `*.epub` only those directly in the root. A pattern names books, not folders: `scary` alone matches nothing; use `scary/**`. A leading `!` excludes, and the last matching pattern wins. A list with no positive pattern (only `!` ones) starts from every book, otherwise from none. Left out, every book under the root.
- **Hidden means missing.** See [api.md](api.md#http-routes-cratesserversrcapirs): hidden books and folders are left out of listings and are 404s, are never deleted, and an upload never reveals them.
- Scopes apply to every role; a new user (an approved sign-up) sees the whole library until the file says otherwise.

- A user id must be a valid username (`validate_user`: 1 to 32 of `a–z`, `0–9`, `-`, `_`), since it's also the name of that user's progress files in `.shelf/`.
- Missing at startup: created with no users (`{}`). An unparseable file, a bad user id, an unknown role name (a typo), or a bad `library_root` or `books` pattern stops the server, and the file is left as it is, rather than quietly giving someone different rights.
- **Hand edits apply without a restart.** Before each login or authenticated request, the server compares the file's mtime with the version it has (`DataFile::refresh`) and reloads it when it changed. A broken edit is logged at `error` once, and the last good version stays in use. Since every write re-reads the file, logins and other writes fail with a 500 until it's fixed.
- **`user_requests`** is a list in the order they were asked for. Names may repeat there; only `users` keys are unique, and approval checks that again (see [api.md](api.md#http-routes-cratesserversrcapirs)). A request added by hand needs an `id` to be approved or denied.
- **The server writes it** on login, logout, a password change, a sign-up, an approval or denial, and when a session is extended (at most once a day each). Every write re-reads the file first, applies the change and drops expired sessions, then writes atomically (a temp file in `/data`, then a rename) with mode `600`. It never writes over a file it can't parse; that request fails with a 500 instead. Maps are written in sorted order (`btree_map` in `crates/proto/build.rs`), pretty-printed, so the file stays readable; unknown fields are dropped when it's rewritten.
- Expired sessions, and sessions whose times don't parse, are ignored on load and removed on the next write.
- The users are never logged, only their count. The frontend's generated code leaves `serverpb` out (`exclude_paths` in `buf.gen.yaml`).

## Logging

All logs go to stdout as one JSON object per line (`tracing-subscriber` with `.json().flatten_event(true).with_current_span(true).with_span_list(false)`). Level comes from `RUST_LOG`, default `info`.

- Every line has `timestamp`, `level`, `target`, `message` (`fields.*` flattened to the top level).
- Lines emitted while handling a request carry a `span` object with that request's `request_id`.
- **Access log**: one line per `/api` request with `target: "access"` and fields `request_id`, `method`, `path`, `status`, `latency_ms`, `req_bytes`, `resp_bytes`, `user` (the signed-in user, or for a login the name it was for; empty when there's none). Nothing about the client's device (IP address, user agent) is logged, by choice. Request IDs come from tower-http (`SetRequestIdLayer` + `PropagateRequestIdLayer`, header `x-request-id`, UUIDs). `req_bytes` is the request's `Content-Length`; `resp_bytes` is the body's exact size, else its `Content-Length`; either is `0` when unknown. `latency_ms` is a float.
- Static frontend requests aren't access-logged.
- **Auth events** (`info`): `logged in`, `login failed: wrong password` (with `user`), `login failed: unknown user` (without the name, which could be a password typed into the wrong field), `logged out`, `password changed`. A lockout is a `warn`. Passwords, tokens, their hashes, `Authorization` headers and `/api/auth/*` bodies are never logged.

```fish
docker logs shelf | jq 'select(.target == "access")'
```

## Docker image

`Dockerfile` (repo root), three stages:

1. **frontend**: `node:22.22.2-trixie-slim` + `buf` 1.75.0 + `pnpm` 12.15.0; `pnpm --dir frontend build` (which runs `buf generate` first).
2. **server**: `rust:1.99.1-slim-trixie`; copies the workspace (including `app/`, only so Cargo can resolve the workspace) and the built `frontend/dist`; `cargo build --release --locked -p shelf-server` with cache mounts for the registry and `target/`.
3. **runtime**: `debian:trixie-slim`, the binary only, runs as **uid/gid 1000**, `VOLUME ["/books", "/data"]` (both created owned by uid 1000), `EXPOSE 8080`.

`.dockerignore` excludes `.git`, `.claude`, `target`, `**/node_modules`, `frontend/dist`, `frontend/src/gen` and `app/gen`.

The library folder mounted at `/books` must be writable by uid 1000, because the server writes `.shelf/` into it. So must the folder mounted at `/data`, where it creates `data.json`.

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

This flow is in use: the home server runs the image pulled from ghcr.

## Deploying with compose

`docker-compose.yml` (repo root) is a sample to copy next to the books folder on the server:

```yaml
services:
  shelf:
    image: ghcr.io/psomani16k/shelf:latest
    container_name: shelf
    restart: unless-stopped
    ports: ["8080:8080"]
    volumes: ["~/notes/books:/books", "./data:/data"] # both writable by uid 1000
    environment: { RUST_LOG: info }
    logging:                          # json-file never rotates by default
      driver: json-file
      options: { max-size: "10m", max-file: "3" }
```

```fish
docker compose pull; and docker compose up -d
docker compose logs -f shelf
```

If a host folder doesn't exist, Docker creates it owned by root and the server can't write `.shelf/` or `data.json`. Create them first, or `sudo chown 1000:1000` them. Adjust the books volume to where the books are; keep `data` outside it.

## Running locally without Docker

The library path is hard-coded to `/books` and the data folder to `/data`, so locally both must exist and be writable by you:

```fish
sudo mkdir -p /books /data; and sudo chown $USER /books /data   # once
pnpm --dir frontend build    # optional: without it the server returns 404 for the UI
cargo run -p shelf-server
```

Every API route needs a login, so add a user to `/data/data.json` (created empty on the first start; no restart needed), e.g. `{ "users": { "dev": { "password": "dev", "role": "OWNER" } } }`.

For frontend work, run the server and `pnpm --dir frontend dev`: Vite proxies `/api` to `http://localhost:8080`.
