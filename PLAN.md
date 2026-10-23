# Shelf — Implementation Plan

> **Working name:** "Shelf" (rename freely; the library data folder is `.shelf/`).
> This document is the source of truth for building the project. It is written to be handed to Claude Code and worked through milestone by milestone.

---

## 0. How to work with this plan (for Claude Code)

- Work **one milestone at a time**, in order. Don't start the next milestone until the current one's acceptance criteria pass.
- Treat **Section 2 (Decisions)** as settled. If something there turns out to be wrong or impossible, stop and explain why before changing course.
- After each milestone: run `cargo fmt`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `buf lint`, and the frontend's `pnpm lint && pnpm test`. If any `.proto` file changed, regenerate code (`pnpm gen:proto`) and run `buf breaking` (see 6.1). Tick the milestone's checkboxes in this file.
- Where this plan says **(verify)**, the exact API was written from memory. Check the library's current docs or source before relying on it.
- Prefer small, boring, well-tested code over clever abstractions. This is a personal app.

---

## 1. What we're building

A cross-platform **EPUB reader** whose library is a **plain folder of books**, in the style of an Obsidian vault:

- Books are `.epub` files anywhere inside a library directory.
- Reading progress and other app data live **inside the library**, in a hidden `.shelf/` folder.
- The same reading experience is available on:
  - **Linux desktop app** (Tauri), 2 laptops
  - **Android app** (Tauri)
  - **Web browser**, served by a **home server** binary that also runs on Linux

### Three ways to use a library

| Mode | Where the books are | How data moves between devices |
|---|---|---|
| **Local** (Tauri app) | Folder on this device | The user's own sync tool (e.g. Syncthing). The app just reads and writes files. |
| **Server** (Tauri app) | On the home server only | App talks to the server over HTTPS, Jellyfin-style. There is no local copy. |
| **Web** (browser) | On the home server only | Browser talks to the server, which serves the same UI. |

All three read and write **the same on-disk format**, so they can be mixed. For example, laptops can use Local mode with Syncthing while the phone uses Server mode against a server whose library folder is also synced.

### Goals

- Same UI and reading behaviour everywhere (one frontend codebase).
- Reading position follows you across devices.
- Works fully offline in Local mode.
- Minimal UI effort: use ready-made components; looks clean by default.

### Non-goals (v1)

- **Sync conflict resolution.** Syncing the folder is the user's sync tool's job. If it creates conflict copies, we ignore them (see 4.4).
- Offline reading in Server mode (planned later, see Milestone 9).
- Formats other than EPUB (foliate-js supports more; add later).
- Windows, macOS and iOS are not targets, but avoid breaking them needlessly.
- Play Store distribution (affects the Android storage approach, see 7.1).
- Multi-user sharing features beyond a simple login.

---

## 2. Decisions (settled)

| Area | Choice | Why |
|---|---|---|
| App shell | **Tauri 2** | Mature Linux and Android support, Rust backend, web UI. |
| Frontend | **React + Vite + TypeScript**, built as a **static SPA** | Tauri needs static assets; the server serves the same build. |
| UI kit | **Tailwind CSS + shadcn/ui** | Good-looking defaults, minimal design work. |
| Data fetching | **TanStack Query** | Caching, refetch on focus, invalidation from file-watch events. |
| Routing | **React Router** (or TanStack Router) | Either is fine; pick one and stay consistent. |
| EPUB rendering | **foliate-js**, vendored as a git submodule pinned to a commit | Battle-tested engine from Foliate: pagination, themes, TOC, CFI positions. |
| Rust EPUB metadata | `epub` crate; fallback to `zip` + `quick-xml` if needed | Only titles, authors, cover and identifier are needed on the Rust side. |
| Server | **axum** + **tower-http**, single binary with the frontend embedded via **rust-embed** | Simple to deploy. |
| API types | **Protocol Buffers** schema in `proto/`. Rust code via **prost** + **pbjson**, compiled with **protox** (pure Rust, no `protoc` install). TypeScript via **protobuf-es** (`buf generate`). | One language-neutral schema with explicit field numbers for safe evolution; ready for clients in other languages later. |
| Wire format | **Binary protobuf** over Tauri IPC and HTTP. The server also speaks proto3 JSON when asked (`Accept: application/json`) for debugging. | Compact and strictly typed; JSON keeps `curl` debugging easy. |
| On-disk format | Stays **hand-written JSON** (serde), **not** protobuf | Library files must stay human-readable, hand-editable, and preserve unknown fields (see 4.3). |
| RPC framework | **None.** Plain REST routes and Tauri commands carrying protobuf messages; no gRPC or Connect. | Keeps the server and Tauri wiring simple. |
| Positions | **EPUB CFI** strings | Stable across screen sizes, fonts and devices. |
| Progress storage | **One file per book**, overwritten by whichever device last read it | Conflicts are out of scope (see Non-goals). |
| Auth (server) | Username + argon2 password hash in server config; opaque session tokens | Personal app; keep it simple but real. |
| App to server HTTP | **tauri-plugin-http** (requests made from Rust) | Avoids CORS and cookie issues in the webview. |
| Android storage | **"All files access"** (`MANAGE_EXTERNAL_STORAGE`) for Local mode | Simplest way to read a synced folder; fine for a sideloaded personal app. |

### Core principle: the frontend never sees file paths

The UI works only with **book IDs**. Every data operation goes through a single `Backend` interface (see Section 6) with two implementations:

- **TauriBackend** calls Tauri commands, which call the Rust `core` crate directly against a local folder.
- **HttpBackend** calls the server's REST API. Both the browser and the app in Server mode use it.

This one seam is what makes Local, Server and Web modes cheap to support.

---

## 3. Repository layout

```
shelf/
├── Cargo.toml                 # Cargo workspace: crates/*, app
├── PLAN.md
├── buf.yaml                   # buf module config (lint + breaking rules)
├── buf.gen.yaml               # TypeScript codegen (protoc-gen-es → frontend/src/gen)
├── proto/
│   └── shelf/v1/
│       └── api.proto          # API schema: source of truth for all wire types
├── crates/
│   ├── core/                  # Library logic. No Tauri, no HTTP.
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── library.rs     # scan, index (id → path), watch
│   │   │   ├── book.rs        # EPUB metadata + cover extraction
│   │   │   ├── id.rs          # book ID derivation
│   │   │   ├── progress.rs    # read/write .shelf/progress/<id>.json
│   │   │   ├── fs.rs          # LibraryFs trait + std implementation
│   │   │   ├── cache.rs       # local (non-synced) metadata cache
│   │   │   ├── types.rs       # domain types (plain Rust; serde only for on-disk files)
│   │   │   └── error.rs
│   │   └── tests/fixtures/    # small public-domain EPUBs for tests
│   ├── proto/                 # crate `shelf-proto`: generated Rust types + conversions
│   │   ├── build.rs           # protox → prost-build + pbjson-build
│   │   └── src/
│   │       ├── lib.rs         # includes generated code
│   │       └── conv.rs        # From/TryFrom between core domain types and proto messages
│   └── server/                # axum binary
│       └── src/
│           ├── main.rs        # CLI: `serve`, `hash-password`
│           ├── config.rs
│           ├── auth.rs
│           ├── routes.rs
│           └── static_files.rs  # rust-embed of frontend/dist + SPA fallback
├── app/                       # Tauri crate (what `tauri init` calls src-tauri)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs             # mobile entry point, plugin setup
│   │   ├── commands.rs        # thin wrappers over core
│   │   └── watcher.rs         # notify → Tauri events
│   └── gen/android/           # generated by `cargo tauri android init`
└── frontend/
    ├── package.json
    ├── vite.config.ts
    ├── vendor/foliate-js/     # git submodule, pinned
    └── src/
        ├── main.tsx
        ├── api/
        │   ├── backend.ts     # Backend interface
        │   ├── tauri.ts       # TauriBackend
        │   ├── http.ts        # HttpBackend
        │   └── index.ts       # picks the backend for the current environment/mode
        ├── gen/               # protobuf-es output (git-ignored; regenerated by dev/build/test; do not edit by hand)
        ├── routes/            # Library, Reader, Settings, Login, Onboarding
        ├── reader/
        │   ├── FoliateView.tsx
        │   ├── useProgressSync.ts
        │   └── readerSettings.ts
        ├── components/ui/     # shadcn components
        └── lib/
```

**Tauri wiring:** in `app/tauri.conf.json` set `build.devUrl` to `http://localhost:5173`, `build.frontendDist` to `../frontend/dist`, and `beforeDevCommand` / `beforeBuildCommand` to run `pnpm --dir ../frontend dev` / `build`. Run Tauri from `app/` with `cargo tauri dev`. For Android dev, the Vite server must listen on `TAURI_DEV_HOST` (see Tauri's mobile Vite template) **(verify)**.

---

## 4. Library on-disk format (spec)

### 4.1 Layout

```
MyLibrary/                       # library root (user-chosen)
├── Author - Title.epub          # books anywhere, any depth
├── Fiction/
│   └── Another Book.epub
└── .shelf/                      # app data, synced along with the books
    ├── library.json             # optional: { "version": 1, "name": "My Library" }
    ├── progress/
    │   └── <book-id>.json
    └── tmp/                     # atomic-write staging; users should ignore it in their sync tool
```

- Scanning walks the root recursively, includes `*.epub` (case-insensitive), and **skips any directory whose name starts with `.`**.
- Symlinks: don't follow directory symlinks in v1 (avoids loops).

### 4.2 Book ID

`id` is a 16-character lowercase hex string:

1. If the OPF declares a `unique-identifier` and its value is non-empty, then `id = hex(sha256("uid:" + trimmed_identifier))[0..16]`.
2. Otherwise, `id = hex(sha256("file:" + file_bytes))[0..16]`.

Rationale: the identifier survives renames, moves and metadata edits (for example in Calibre). The content hash is the fallback. Known edge case: two files with the same identifier share an ID; log a warning and keep the first one found (sorted by relative path).

### 4.3 Progress file: `.shelf/progress/<book-id>.json`

```json
{
  "version": 1,
  "cfi": "epubcfi(/6/14!/4/2/10:0)",
  "fraction": 0.4213,
  "chapter": "Chapter 12",
  "updated": "2026-10-23T16:05:00Z",
  "device": "phone"
}
```

- `fraction` (0–1) and `chapter` let the library view show progress without opening the book.
- `updated` is an RFC 3339 UTC timestamp set by the writer. `device` is the writer's device name.
- **Single file per book; last writer wins.** No merging.
- Unknown fields must be preserved on rewrite (read into a struct with `#[serde(flatten)] extra: Map`) so future versions don't lose data.
- This is a **file format, not an API type**. It is a hand-written serde struct in `core`, deliberately **not** protobuf, so users and sync tools see plain, editable JSON. `core` converts it to and from the domain `Progress` type, and `shelf-proto` converts that to the wire message.

### 4.4 Writing and reading rules

- **Atomic writes:** write to `.shelf/tmp/<random>.tmp`, `fsync`, then `rename` onto the target.
- **Ignore what you don't recognise:** in `.shelf/progress/`, only read files that exactly match `^[0-9a-f]{16}\.json$`. Skip parse errors with a logged warning. Never delete or rename unknown files; sync-tool conflict copies are the user's business.
- Create `.shelf/`, `progress/` and `tmp/` on first write, not on scan, so a read-only browse doesn't modify the library.

### 4.5 What is NOT stored in the library

These are device-local and must **not** be written into the library folder:

- Metadata and cover thumbnail cache (OS cache dir; server uses its `state_dir`).
- App configuration: mode, library path, server URL, token, device name.
- Reader display preferences (font size and so on). These are per-device, because a phone and a laptop want different sizes.

---

## 5. Rust `core` crate

### 5.1 Responsibilities

- Open a library at a root path; scan and build an index `id → (rel_path, metadata)`.
- Extract metadata: title, authors, language, description, publisher, published date, cover image bytes plus MIME type.
- Cache parsed metadata keyed by `(rel_path, size, mtime)` in a cache dir supplied by the caller, so rescans are fast.
- Read and write progress per Section 4.
- Watch the library (`notify` crate, debounced around 500 ms) and report changes as `LibraryEvent::{BooksChanged, ProgressChanged(id)}` through a callback or channel.

### 5.2 Filesystem seam

Define a small trait so Android's Storage Access Framework could be added later without touching logic:

```rust
pub trait LibraryFs: Send + Sync {
    fn walk_epubs(&self) -> Result<Vec<FileEntry>>;          // rel_path, size, mtime
    fn read(&self, rel_path: &str) -> Result<Vec<u8>>;
    fn read_optional(&self, rel_path: &str) -> Result<Option<Vec<u8>>>;
    fn write_atomic(&self, rel_path: &str, bytes: &[u8]) -> Result<()>;
}
```

Only `StdFs` (plain `std::fs`) is implemented in v1.

### 5.3 Shared types (`types.rs`, exported with ts-rs)

```rust
LibraryInfo   { name: String, book_count: u32 }
BookSummary   { id, title, authors: Vec<String>, rel_path: String, has_cover: bool,
                progress: Option<ProgressSummary> }
ProgressSummary { fraction: f64, chapter: Option<String>, updated: String, device: String }
BookDetail    { summary fields + language, description, publisher, published: Option<String> }
Progress      { cfi: String, fraction: f64, chapter: Option<String>, updated: String, device: String }
ProgressInput { cfi: String, fraction: f64, chapter: Option<String>, device: String }
ApiError      { code: ErrorCode, message: String }  // NotFound, BadRequest, Unauthorized, Io, Internal
```

`rel_path` is for display and grouping by folder only. The frontend never uses it for I/O.

Export the TS types to `frontend/src/generated/` and commit them. Add a CI or test check that regenerating them produces no diff.

### 5.4 Public API (sketch)

```rust
impl Library {
    pub fn open(root: PathBuf, cache_dir: PathBuf) -> Result<Self>;
    pub fn info(&self) -> LibraryInfo;
    pub fn list_books(&self) -> Vec<BookSummary>;
    pub fn book(&self, id: &str) -> Result<BookDetail>;
    pub fn book_bytes(&self, id: &str) -> Result<Vec<u8>>;
    pub fn cover(&self, id: &str) -> Result<Option<(Vec<u8>, String)>>;  // bytes, mime
    pub fn progress(&self, id: &str) -> Result<Option<Progress>>;
    pub fn save_progress(&self, id: &str, input: ProgressInput) -> Result<Progress>; // sets `updated`
    pub fn rescan(&self) -> Result<()>;
    pub fn watch(&self, on_event: impl Fn(LibraryEvent) + Send + 'static) -> Result<WatchHandle>;
}
```

Internally use `RwLock` for the index; `Library` must be `Send + Sync` and shareable via `Arc`.

---

## 6. The `Backend` interface (frontend)

```ts
// frontend/src/api/backend.ts
export interface Backend {
  kind: "tauri-local" | "http";
  libraryInfo(): Promise<LibraryInfo>;
  listBooks(): Promise<BookSummary[]>;
  getBook(id: string): Promise<BookDetail>;
  getBookFile(id: string): Promise<Blob>;          // fed straight into foliate-js
  getCover(id: string): Promise<Blob | null>;      // rendered via object URLs
  getProgress(id: string): Promise<Progress | null>;
  saveProgress(id: string, input: ProgressInput): Promise<Progress>;
  /** Push notifications of changes; HTTP backend may return a no-op. */
  subscribe(cb: (e: LibraryEvent) => void): () => void;
}
```

- Covers are fetched as blobs (not `<img src>` URLs) so authenticated Server mode works the same as everything else. Cache them with TanStack Query.
- **Selecting a backend (`api/index.ts`):**
  - Not in Tauri (`isTauri()` from `@tauri-apps/api/core` **(verify)**): use `HttpBackend` with a same-origin base URL and cookie auth.
  - In Tauri with mode `local`: use `TauriBackend`.
  - In Tauri with mode `server`: use `HttpBackend` with the configured base URL, `fetch` from `@tauri-apps/plugin-http`, and bearer-token auth.
- `HttpBackend` takes `{ baseUrl, fetchImpl, auth: "cookie" | { bearer: string } }` so one class serves both the browser and Server mode.

---

## 7. Tauri app (`app/`)

### 7.1 Commands (thin wrappers over `core`)

| Command | Returns |
|---|---|
| `library_info` | `LibraryInfo` |
| `list_books` | `BookSummary[]` |
| `get_book(id)` | `BookDetail` |
| `book_file(id)` | raw bytes via `tauri::ipc::Response` (arrives as `ArrayBuffer`) |
| `book_cover(id)` | raw bytes, or a `NotFound` error |
| `get_progress(id)` | `Progress \| null` |
| `save_progress(id, input)` | `Progress` |
| `get_app_config` / `set_app_config` | `AppConfig` |
| `open_library(path)` | `LibraryInfo`; (re)opens and starts the watcher |

Errors serialize to `ApiError`.

**Watcher:** when the library emits `LibraryEvent`, emit a Tauri event `library-event`. `TauriBackend.subscribe` listens to it, and the frontend invalidates the relevant queries.

### 7.2 App config (device-local, via `tauri-plugin-store`)

```ts
AppConfig {
  mode: "local" | "server" | null;     // null → onboarding
  libraryPath?: string;                // local mode
  serverUrl?: string;                  // server mode
  token?: string;                      // server mode (plaintext store is acceptable for v1)
  deviceName: string;                  // default: hostname, or "android"
}
```

### 7.3 Plugins and permissions

- `tauri-plugin-store`: app config.
- `tauri-plugin-dialog`: folder picker for Local mode on desktop.
- `tauri-plugin-http`: Server mode requests. The server URL is user-supplied, so the capability scope must allow `https://*` (and `http://*` for LAN testing) **(verify the exact capability syntax)**.
- Keep capabilities minimal otherwise.

### 7.4 Android specifics

- **Storage (Local mode):** add `MANAGE_EXTERNAL_STORAGE` to the manifest. On first launch in Local mode, if access isn't granted, show a screen explaining why and open the system "All files access" settings page. This needs either a tiny Kotlin plugin or the opener plugin with the right intent **(verify)**. Then `StdFs` works on paths like `/storage/emulated/0/Books`.
- Folder selection on Android: a text field with the path plus a few suggestions (`/storage/emulated/0/...`). A native picker is a later nice-to-have.
- Server mode needs no storage permission.
- Use HTTPS for the server. Android restricts cleartext traffic by default.
- The app must survive backgrounding: save progress on `visibilitychange → hidden` (see 9.3).

---

## 8. Server (`crates/server`)

### 8.1 CLI and config

```
shelf-server serve --config /etc/shelf/config.toml
shelf-server hash-password          # prompts, prints an argon2id hash
```

```toml
library   = "/srv/books"
bind      = "127.0.0.1:8080"
state_dir = "/var/lib/shelf"        # metadata cache + sessions; NOT inside the library
device_name = "web"                 # default `device` for progress written via the web UI

[[users]]
name = "me"
password_hash = "$argon2id$v=19$..."
```

### 8.2 Auth

- `POST /api/auth/login {username, password}` checks the argon2 hash and creates a random 32-byte token. Store only `sha256(token)` plus user and expiry (90 days, sliding) in `state_dir/sessions.json`. Respond with `{ token }` **and** set an `HttpOnly; Secure; SameSite=Lax` cookie.
- Requests authenticate with **either** the cookie (browser) **or** `Authorization: Bearer <token>` (app).
- `POST /api/auth/logout` revokes the token. `GET /api/auth/me` returns the username or 401.
- Rate-limit login attempts per IP (simple in-memory counter).
- Every `/api/*` route except `login` and `health` requires auth.

### 8.3 REST API

| Method and path | Body / response |
|---|---|
| `GET /api/health` | `200 ok` (no auth) |
| `GET /api/library` | `LibraryInfo` |
| `GET /api/books` | `BookSummary[]` |
| `GET /api/books/{id}` | `BookDetail` |
| `GET /api/books/{id}/file` | EPUB bytes, `application/epub+zip`, supports `Range` |
| `GET /api/books/{id}/cover` | image bytes or 404 |
| `GET /api/books/{id}/progress` | `Progress` or 404 |
| `PUT /api/books/{id}/progress` | `ProgressInput` → `Progress` |

- Errors are returned as an `ApiError` JSON body with a matching status code.
- The server keeps one `Library` open and runs the watcher so its index stays fresh when the folder is synced from elsewhere.
- Concurrent writes from different clients: last write wins (consistent with the spec).

### 8.4 Static frontend

- Embed `frontend/dist` with `rust-embed`. Any non-`/api` path that doesn't match a file serves `index.html` (SPA fallback).
- Long cache headers for hashed assets; `no-cache` for `index.html`.
- Document that `frontend` must be built before `cargo build -p shelf-server --release` (or add a `build.rs` check with a clear error).

### 8.5 Deployment (documented, not automated)

- Bind to localhost and expose via **Tailscale** (`tailscale serve` gives HTTPS) or a reverse proxy such as **Caddy** with automatic HTTPS.
- Provide an example `systemd` unit in `docs/`.

---

## 9. Frontend

### 9.1 Screens

- **Onboarding** (Tauri only, when `mode` is null): choose "Open a folder on this device" or "Connect to a server". Collect the folder path, or the URL plus login. Set the device name.
- **Login** (web, and Server mode when the token is missing or invalid).
- **Library:** a grid of covers with title, author and a progress bar. Search by title or author. Sort by recently read, title or author. Optional grouping by folder (from `rel_path`). Pull-to-refresh or a refresh button.
- **Reader:** full-screen foliate-js view with a minimal top bar (back, title, TOC, settings) that auto-hides. Tap or click zones and arrow keys turn pages. Shows chapter name and percentage.
- **Settings:** mode and connection (Tauri), device name, reader preferences, and an "about / library info" section.

### 9.2 Reader (foliate-js)

- Wrap foliate-js's `<foliate-view>` custom element in a React component. Open the book from the `Blob` returned by `getBookFile` **(verify the current foliate-js API: `view.open(file)`, the `relocate` event with `detail.cfi` / `detail.fraction` / `detail.tocItem`, and `view.init({ lastLocation })` or `view.goTo(cfi)`)**. Use foliate-js's own `reader.js` as the reference integration.
- Reader preferences are stored per device (localStorage on web; the app store in Tauri): theme (light, dark, sepia, follow system), font family, font size, line height, margins, and paginated vs scrolled. Apply them through the renderer's style API **(verify)**.
- Keyboard: ←/→ and PageUp/PageDown, Space. Touch: tap zones and swipe.

### 9.3 Progress sync behaviour (`useProgressSync`)

- **On open:** fetch progress. If present, open at its `cfi`. If it came from another device, show a toast such as "Resumed where you left off on *phone* · 3 min ago".
- **While reading:** on each `relocate`, keep the latest position in memory and save it **debounced (about 2 s after the last change)**.
- **Flush immediately** on: closing the book, route change, `visibilitychange → hidden`, and `pagehide`.
- **On return:** on `visibilitychange → visible` / focus (and on `ProgressChanged` events in Local mode), refetch progress. If it's newer and from a different device, show a non-blocking prompt: "Jump to where you were on *laptop-2* (Chapter 14)?" Never jump automatically mid-read.
- Don't save a position until the book has finished its initial navigation, so opening a book can't overwrite progress with "start of book".

### 9.4 Query behaviour

- TanStack Query with `refetchOnWindowFocus: true` for the book list and progress.
- In Local mode, `library-event` invalidates `["books"]` and `["progress", id]`.

---

## 10. Milestones

Each milestone ends with its acceptance criteria passing and the checklist ticked.

### M0 — Android storage spike *(do first; de-risks Local mode on Android)*
- [ ] Minimal Tauri 2 app builds and runs on the user's Android phone.
- [ ] With "All files access" granted, Rust `std::fs` can **list, read and atomically write** files in a folder that a sync tool also writes to (e.g. a Syncthing folder under `/storage/emulated/0/`).
- [ ] Write up findings in `docs/android-storage.md`. If this approach fails, stop and report the options (for example a SAF-based Kotlin plugin implementing `LibraryFs`) before continuing.

### M1 — Workspace + `core`
- [ ] Cargo workspace, `crates/core`, CI script (fmt, clippy, test).
- [ ] Scan, index, book ID (4.2), metadata and cover extraction, metadata cache.
- [ ] Progress read and write per 4.3–4.4, including atomic writes, preservation of unknown fields, and ignoring unrecognised files.
- [ ] Watcher producing debounced `LibraryEvent`s.
- [ ] ts-rs type export into `frontend/src/generated/`.
- [ ] Tests with 2–3 small public-domain EPUBs (one without a unique identifier, one without a cover), covering progress round-trips, conflict-copy files being ignored, and rename stability of IDs.

### M2 — Frontend skeleton + Tauri Local mode (Linux)
- [ ] Vite + React + TS + Tailwind + shadcn/ui set up; foliate-js submodule added and pinned.
- [ ] `Backend` interface, `TauriBackend`, and backend selection.
- [ ] Tauri app with the commands in 7.1, app config store, folder picker, and onboarding for Local mode.
- [ ] Library screen showing books with covers and progress.
- [ ] Reader opens a book; progress is saved and restored per 9.3.
- **Accept:** pick a folder on Linux, open a book, read a few pages, quit, and reopen at the same spot. Edit the progress file by hand and the UI reflects it.

### M3 — Reader polish
- [ ] TOC panel, chapter title and percentage display, keyboard and touch navigation.
- [ ] Reader preferences (theme, font, size, spacing, margins, paginated/scrolled), stored per device.
- [ ] Resume toast and "jump to newer position" prompt (9.3); file-watch-driven refresh.
- [ ] App-wide light and dark theme following the system.
- **Accept:** two Linux machines sharing a folder via Syncthing: reading on one, then focusing the app on the other, offers the newer position.

### M4 — Server + Web
- [ ] `shelf-server` with config, `hash-password`, auth (8.2), REST API (8.3), Range support, and the watcher.
- [ ] `HttpBackend` (cookie mode) and a Login screen.
- [ ] Embedded frontend with SPA fallback.
- [ ] Integration tests for auth and every route (spin the server up on a temp library).
- [ ] `docs/deploy.md` with Tailscale/Caddy and systemd examples.
- **Accept:** from a browser on another machine, log in, browse, read, and have progress written into the server's library folder in the same format as Local mode.

### M5 — Server mode in the Tauri app
- [ ] Onboarding and Settings support "Connect to a server" (URL, login, then a stored token).
- [ ] `HttpBackend` in bearer mode using `tauri-plugin-http`; capability scope configured.
- [ ] Switching modes in Settings without reinstalling. Expired or invalid tokens go back to Login.
- **Accept:** a Linux app in Server mode and a browser show the same progress for the same book.

### M6 — Android
- [ ] Android build of the app with Local mode (using M0's approach) and Server mode.
- [ ] Touch-friendly layout check across all screens; safe-area insets; the back button behaves sensibly.
- [ ] Progress flushes when the app is backgrounded.
- **Accept:** read on the phone, pick up a laptop, and continue at the same place, in both Local+Syncthing and Server setups.

### M7 — Packaging
- [ ] Linux: AppImage and .deb via `cargo tauri build`.
- [ ] Android: signed release APK (document keystore setup in `docs/android-release.md`).
- [ ] Server: release binary and example config.
- [ ] Top-level `README.md` with setup for all three modes, including a recommendation to ignore `.shelf/tmp/` in the sync tool.

### Later (not v1, in rough priority order)
- **M8 — Annotations:** bookmarks and highlights in `.shelf/annotations/<book-id>.json`.
- **M9 — Offline reading in Server mode:** download books to a local cache and queue progress writes while offline.
- **M10 — More formats:** MOBI/AZW3, FB2 and CBZ via foliate-js; metadata support in `core`.
- Full-text search across the library; collections and tags; reading stats; native Android folder picker or SAF support.

---

## 11. Testing strategy

- **core:** unit tests per module plus fixture-based tests (Section 10, M1). Property-style test: write progress, read it back, and get an identical result including unknown fields.
- **server:** integration tests against a temp library, covering auth flows, 401s, Range requests and progress round-trips.
- **frontend:** Vitest for `HttpBackend` (mocked fetch), backend selection, and the debounce and flush logic in `useProgressSync`.
- **End-to-end (optional, after M4):** a Playwright smoke test against the web build: log in, open a book, turn a page, reload, and check the position persisted.
- Fixture EPUBs: small public-domain titles (Project Gutenberg / Standard Ebooks), trimmed to keep the repo light.

---

## 12. Risks and open questions

| Item | Notes |
|---|---|
| Android "All files access" | Works for sideloading; Google Play would likely reject it. If Play distribution ever matters, implement a SAF-backed `LibraryFs` via a Kotlin plugin. |
| WebKitGTK performance on Linux | Check page-turn smoothness early in M2. foliate-js is generally fine, but heavy CSS animations in the UI should be avoided. |
| Book ID collisions | Same identifier across different editions maps to one ID. Accepted for v1; revisit if it bites. |
| Clock skew | `updated` uses the writer's clock and is only for display. Last-write-wins is enforced by the user's sync tool and filesystem, not by us. |
| foliate-js API drift | Pinned submodule; upgrade deliberately. |
| Token storage | Plaintext in the app store for v1; consider OS keyring or Stronghold later. |
| App name | "Shelf" is a placeholder. Renaming touches the crate names, bundle identifier and the `.shelf/` folder name, so decide before M7. |
