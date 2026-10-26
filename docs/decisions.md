# Goals and decisions

What Shelf is for, the choices it's built on, and where the original plan (the old `PLAN.md`, removed in commit `aed1ede`) was changed. Unbuilt plans are in [roadmap.md](roadmap.md).

## What it's for

A personal EPUB reader whose library is a plain folder of books, like an Obsidian vault. One user, on two Linux laptops, an Android phone and a browser.

### Ways to use a library

| Mode | Where the books are | How data moves between devices |
|---|---|---|
| **Local** (app) | A folder on the device | The user's own sync tool (e.g. Syncthing). Shelf just reads and writes files. |
| **Remote** (app) | On the server only | The app talks to `shelf-server`, Jellyfin-style. Downloaded books and covers are cached; nothing else is local. |
| **Web** (browser) | On the server only | The browser talks to the server, which serves the same UI. |

All three use the same on-disk format, so they can be mixed: laptops in Local mode on a Syncthing folder, the phone in Remote mode against a server whose library is the same synced folder.

### Goals

- The same UI and reading behaviour everywhere, from one frontend codebase.
- Reading position follows you across devices.
- Local mode works fully offline.
- Minimal UI effort: ready-made components that look clean by default.

### Non-goals

- **Sync conflict resolution.** Syncing the folder is the sync tool's job.
- **Live syncing between devices in use at the same time.** One user, one device at a time: progress is read when a book opens. No "progress changed" events and no "jump to newer position" prompts.
- Native Windows, macOS and iOS apps (but don't break them needlessly). Distributing them without warnings needs paid signing: Apple's $99/year developer program (iOS also needs a Mac to build, and free installs expire after 7 days), and Windows code signing (Microsoft's Artifact Signing is open to individuals only in the US and Canada). Not worth it for a personal project. On those devices, use the web UI from `shelf-server` in a browser.
- Play Store distribution. This is what allows the Android all-files-access approach.
- Multi-user features beyond a simple login (and the login isn't built yet).

## Decisions

| Area | Choice | Why |
|---|---|---|
| App shell | Tauri 2 | Mature Linux and Android support, Rust backend, web UI |
| Frontend | React + Vite + TypeScript, built as a static SPA | Tauri needs static assets; the server serves the same build |
| UI kit | Tailwind CSS + shadcn/ui (Base UI, Nova preset) | Good defaults, little design work |
| Data fetching | TanStack Query | Caching and invalidation, including from watcher events |
| Routing | React Router (`BrowserRouter` on the web, `MemoryRouter` in the app) | — |
| EPUB rendering | foliate-js, a git submodule pinned to a commit | Battle-tested: pagination, themes, TOC, CFI positions |
| Rust EPUB parsing | `rbook` | Title, authors and cover are all the Rust side needs |
| Server | axum + tower-http, one binary with the frontend embedded via rust-embed, shipped as a Docker image | Simple to deploy on a home server |
| Schema | Protocol Buffers in `proto/`, used only as a language-neutral schema: prost + pbjson (compiled with protox, no `protoc`) for Rust, protobuf-es for TypeScript | One source for every type in every language |
| Wire format | proto3 JSON with snake_case proto field names, over HTTP and Tauri IPC; never binary protobuf. EPUB and cover bytes go raw | Readable in devtools and `curl`; all generated clients agree |
| RPC | None: plain routes and Tauri commands carrying JSON proto messages | Keeps both sides simple |
| Positions | EPUB CFI strings | Stable across screen sizes, fonts and devices |
| Progress storage | One file per book, last writer wins | Conflicts are out of scope |
| Book ID | SHA-1 of the file's bytes | Stable across moves and renames; needs nothing from the EPUB |
| Extracted metadata | Stored in the library's `.shelf/`; deterministic apart from `last_update_time` | Each book is extracted once, not on every device, and syncs with the library |
| Library settings | `.shelf/config.json`, merged by partial patches | Settings follow the library to every device |
| App-to-server HTTP | From Rust (`reqwest` via tauri-plugin-http), behind the same `Library` trait as local folders | No CORS or cookie issues in the WebView; the frontend has one backend per platform |
| Covers and books in the app | Raw bytes over IPC (`ipc::Response`) → Blob → object URL | WebKitGTK won't load custom URI schemes from the `http://` dev page |
| Android storage | All files access (`MANAGE_EXTERNAL_STORAGE`) and a small Kotlin plugin, with plain `std::fs` paths | The simplest way to read a synced folder; fine for a sideloaded app |
| Server auth (planned) | Username + argon2 hash in config; opaque session tokens | Personal app: simple but real. See [roadmap.md](roadmap.md#authentication) |

### Naming and JSON conventions

- Messages are CapitalCamelCase; fields are snake_case in the schema **and on the wire** (`preserve_proto_field_names()` in pbjson-build, `useProtoFieldName: true` in protobuf-es).
- Enums sit in a wrapper message (`ReaderTheme.Value`) so their values are plain names (`LIGHT`), sent by name. `buf.yaml` turns off the prefix rules for that.
- Avoid 64-bit integers: proto3 JSON sends them as strings. Use `int32` / `uint32`, or `double` for fractions.
- Fields holding their default value are left out of the JSON. Always decode with generated code (`fromJson`, serde), never raw `JSON.parse` output.
- Unknown fields are ignored when decoding (`ignore_unknown_fields()`, `ignoreUnknownFields: true`), so an older client keeps working against a newer server.
- JSON field names are part of the contract: renaming a field breaks clients even if its number stays the same. Never renumber or reuse a field number.
- Scalar and enum fields in `Config` are `optional`, so a partial `set_config` can tell "not sent" from the zero value.

### The frontend never sees file paths

The UI works only with book IDs, through one `Backend` interface with an HTTP and a Tauri implementation. That one seam is what makes the three modes cheap to support. (The app's library picker is the exception: it sends the chosen folder to Rust.)

## Where the plan changed

| The plan said | What was built, and why |
|---|---|
| Book ID from the OPF `unique-identifier` (SHA-256, 16 hex chars), file hash as fallback | SHA-1 of the file, always. Simpler; the cost is that editing a book changes its ID (see [roadmap.md](roadmap.md)) |
| Metadata cache outside the library, keyed by path/size/mtime | Metadata in `.shelf/books/<id>/`, so it's extracted once and syncs. Startup skips hashing a book whose mtime isn't newer than its `last_update_time` (the time this device last hashed it), at the cost of `metadata.json` differing per device |
| Progress at `.shelf/progress/<id>.json` with `fraction`, `chapter`, `updated`, `device` | `.shelf/books/<id>/progress.json` with the CFI only |
| Hand-written serde structs for on-disk files, preserving unknown fields | The generated proto types, via pbjson. Only `config.json` keeps unknown fields |
| Atomic writes staged in `.shelf/tmp/` | Temp file next to the target, then rename (same filesystem, nothing to ignore in sync tools). Only `state.json` and `config.json` so far |
| Reader preferences per device (localStorage / app store) | Library-wide in `.shelf/config.json`. Per-device preferences are on the roadmap |
| `ProgressChanged` events, resume toast, "jump to newer position" | Dropped: one device at a time |
| App in Server mode uses the frontend's `HttpBackend` with bearer tokens | The app's Rust `RemoteLibrary` talks to the server; the frontend always uses `tauriBackend` in the app |
| REST routes like `GET /api/books`, `PUT /api/books/{id}/progress` | `POST` routes with JSON bodies, one per `Library` method, mirrored by Tauri commands ([api.md](api.md)) |
| Watcher debounce ~500 ms, events `BooksChanged` / `ProgressChanged` | 5 s per path; a single `library-changed` event |
| A `LibraryFs` trait so Android SAF could be added later | Plain paths; SAF only if Play Store distribution ever matters |
| Android folder as a typed path | A native folder picker in `ShelfPlugin.kt` |
| Server binary with a config file, bound to localhost behind Tailscale/Caddy | Docker image, `/books` and `0.0.0.0:8080` hard-coded, no config file yet |
| `.shelf/` created on first write, so browsing doesn't modify the library | Opening a library extracts metadata into `.shelf/` straight away |

## Risks

| Item | Notes |
|---|---|
| Android all-files access | Fine for sideloading; the Play Store would likely reject it. If that ever matters, Android needs a SAF-based file layer. |
| WebKitGTK on Linux | Already laggy on the developer's hybrid-GPU Wayland machine ([app.md](app.md#linux)). Avoid heavy CSS animation. |
| Book ID changes on edit | Editing a file (e.g. metadata in Calibre) gives it a new ID and drops its progress at the next startup. |
| foliate-js API drift | Pinned submodule; upgrade deliberately. |
| App name | "Shelf" was a placeholder. Renaming touches crate names, the bundle identifier and the `.shelf/` folder name; the bundle ID can't change without Android treating it as a new app. |
