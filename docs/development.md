# Development

## Setup

New contributors: start with [CONTRIBUTING.md](../CONTRIBUTING.md), which covers prerequisites on any Linux distro. This section is the maintainer's own machine.

Fedora 44, fish shell. Commands here avoid bash-only syntax (no heredocs, no `export`; chain with `; and`).

```fish
git clone --recursive <repo>          # or, in an existing clone:
git submodule update --init           # fetches frontend/vendor/foliate-js
pnpm --dir frontend install
```

Needs: Rust (edition 2024), Node + pnpm, `buf` (used by `pnpm gen:proto`), the Tauri CLI (`cargo tauri`). When a `-sys` crate can't find a system library: `dnf provides 'pkgconfig(<name>)'` to find the `-devel` package.

Android: SDK at `~/Android/Sdk` with `platforms;android-37.0`, `build-tools;37.0.0`, NDK `29.0.13846066`; environment variables are set in `~/.config/fish/config.fish`. JDK 25 at `/usr/lib/jvm/java-25-openjdk` (Gradle 9.6.1).

## Running

Run from the repo root unless noted.

| What | Command |
|---|---|
| Web UI against a local server | `cargo run -p shelf-server` (library at `/books`, server data at `/data`; both must exist and be writable, see [server.md](server.md#running-locally-without-docker)), then `pnpm --dir frontend dev` → http://localhost:5173 |
| Linux app | `cd app; and cargo tauri dev` |
| Android app on a USB phone | `cd app; and cargo tauri android dev` |
| Android release APK | `cd app; and cargo tauri android build --apk --target aarch64` (see [app.md](app.md)) |
| Server image | `docker build -t ghcr.io/psomani16k/shelf:<version> -t ghcr.io/psomani16k/shelf:latest .` (see [server.md](server.md)) |

## Checks

```fish
cargo fmt; and cargo clippy --workspace -- -D warnings; and cargo test --workspace
cargo clippy -p shelf-core --features s3 -- -D warnings; and cargo test -p shelf-core --features s3
buf lint
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend exec tsc -b
```

The `s3` feature (the S3 storage backend) is off by default, so the workspace commands don't build it; the second line does. Its `aws-lc-rs` dependency needs a C toolchain (`sudo dnf install cmake clang` if `aws-lc-sys` fails to build).

`shelf-core` has pre-existing clippy warnings, so `--workspace -D warnings` fails on them. To lint just the app: `cargo clippy -p app --no-deps -- -D warnings`.

Rust tests: in `crates/core/src/` — `storage/mod.rs` (the config merge), `storage/local_storage.rs` (relative book listing, config read and merge, per-user state, book-data writes leaving no temp files, leftover temp-file cleanup, `clean_up_progress` deleting only folders without `metadata.json` that aren't kept), `storage/s3_storage.rs` (only with `--features s3`, against `object_store`'s `InMemory`: book listing, gone books, book data round trips and cleanup, state and config), `ebook_ingest.rs` (`predict_id_change_using_modified_time` against files with a set mtime, `extract(false)` adding only this copy's location and writing nothing, a gone book having no metadata, and the cover page's image `src` and its path resolution), `routines/extractor.rs` (the startup sync's skipped, failed and removed counts, every copy becoming a location and gone ones dropped, a gone book keeping only its folder and progress (and not counted again), unchanged copies skipped on the next start, a forced sync keeping the progress of a book that fails, a corrupt `metadata.json` keeping its folder), `routines/index_sync.rs` (a renamed folder moving its books, a move in either event order, two copies as one book and deleting one keeping it, an edited copy becoming another book, a gone folder dropping only its own books, a gone non-book file changing nothing), `library/scope.rs` (the root joined and stripped, last match wins, `*` within a folder, exclusion-only lists, bad roots and patterns, hidden folders), `library/view.rs` (a scoped user's listings, 404s and hidden parent folder, a copy uploading as another location, `(n)` names for hidden clashes and a 409 for a visible one, a folder delete keeping hidden books, one copy deleted and a hidden copy skipped, `clean_up_progress` deleting a removed book's folder and keeping an indexed one whose `metadata.json` is gone) and `watcher.rs` (which paths `is_relevant` lets through); in `crates/utility/src/` — `timeutil.rs` (RFC 3339 formatting and parsing), `routines/concurrency.rs` (result order, the `in_flight` cap, errors kept at their index, panics re-raised, for both the async and blocking variants) and `routines/debounced.rs` (a repeat resetting its item's timer, separate timers per item, pending items running after the channel closes, the blocking variant's job cap); in `app/src/` — `app_data.rs` (library name and log-file count validation) and `logging/files.rs` (start-time names, pruning to N without touching the current file, listing, reading one file by name only, export). In `crates/server/src/` — `data.rs` (creating, loading and refusing bad files, a bad library root or pattern included, roles by name with a missing one meaning `BANNED`, sessions round-tripping and expiring, `update` keeping a hand edit, a broken edit never written over, `refresh` picking up an edit), `auth.rs` (login, wrong password, unknown user, a user without a password, the lockout and its reset, sliding expiry written at most once a day, logout, a password change ending the other sessions, a hand-edited role applying at once, a banned user's login refused and a hand-made ban ending the session), `auth/requests.rs` (sign-up refusing a taken or bad name or password without writing, the pending cap, a pending login refused until approval, a second request for the same name unapprovable, only an owner giving `ADMIN`, deny), `auth/users.rs` (an admin promoting, banning and unbanning a reader, a ban ending the sessions, admins unable to touch admins or owners or give `ADMIN`, an owner making and banning an admin, no one changing their own role, unknown users, the list sorted by name, scopes stored normalized, bad roots and patterns as 400s, scopes following the role rule and never your own) and `api.rs` (the router in-process with `tower::ServiceExt::oneshot`, like Go's `httptest`: 401s without a session, covers included, 403s by role, `me` and logout, sign-up without a session and approval by the owner, a ban signing the user out at once, a scoped user listing their folder as the library, and admins setting scopes from the whole library: 403s for a reader, a missing root as a 400, the change applying on the user's next request, and *Clean up progress* for admins only).

## The `demo` crate

`crates/demo` is a small binary for running `core` and `utility` routines by hand against a real folder. `cargo run -p demo` with no arguments lists its functions:

- `force_extract <root>`: runs the startup sync with `force` on `<root>`, as *Rebuild index* does. Every book is re-extracted into `<root>/.shelf/`, and the metadata and thumbnails of books no file produced are deleted (their folders and progress are kept). Prints the `SyncReport`.
- `debounce <delay> <blocking>`: replays a scripted burst of file events through `Debounced` with a `<delay>`-second debounce, on the blocking pool when `<blocking>` is `true`.

Logs go to stdout as JSON lines with the server's flags (`RUST_LOG`, default `info,shelf_core=debug`), followed by the return value printed with `{:?}`. To see only warnings, such as failed extractions:

```fish
cargo run -p demo -- force_extract <root> | jq -R 'fromjson? | select(.level == "WARN")'
```

A new function is a file in `src/commands/` holding one `command!` method, added to `commands!(…)` in `src/commands/mod.rs`. Arguments are parsed with `FromStr`.

## Working with this user

- They're a backend engineer (mostly Go) who knows Rust outside web work, and is using the project to learn frontend/app development and Rust servers. Claude writes the code, explaining the *why* briefly, with Go comparisons where they help.
- **They run the Rust checks and builds themselves.** Don't run `cargo fmt`, `cargo clippy`, `cargo build`/`test` or Docker builds unless asked; say which command to run instead. Frontend checks (`tsc`, oxlint, vitest, vite build) are fine to run.
- **Edit only the files a request names.** Suggest wider refactors in chat instead of making them.
- Do chores (config, docs, mechanical fixes) directly.
- Few code comments: only a non-obvious "why" or a trap, in one line.
- Don't edit `frontend/vendor/foliate-js`, `frontend/src/gen/`, or generated Rust in `target/`.
- Never print or commit the Android keystore passwords (`app/gen/android/keystore.properties`).

## Code conventions

- **tokio everywhere**: async APIs, `tokio::spawn`, `tokio::sync`, `tokio::time`, `tokio::fs`. Blocking or CPU-heavy work (EPUB/ZIP parsing, hashing, image resizing) goes in `spawn_blocking`. Timing tests use `#[tokio::test(start_paused = true)]`. In `core` and the app, get time from `shelf_utility::timeutil` (`crates/utility/src/timeutil.rs`): `Instant`, `sleep`, `sleep_until` and `timeout` are tokio's monotonic clock, so a paused test controls them; use them for timeouts and debouncing. `Time` is a wall-clock timestamp (wraps `SystemTime`): `unix_nanos()` for things like unique file names, `Display` / `FromStr` for anything stored or shown: it writes RFC 3339 in UTC to the whole second (`2025-10-26T04:45:00Z`, sortable as text) and parses any RFC 3339 timestamp back, via `jiff`.
- **Errors**: `anyhow` inside, `LibraryError` at the `Library` boundary, logged once where they leave Rust (`crates/server/src/error.rs`, `app/src/error.rs`).
- **Logging**: `tracing` with structured fields (`info!(book_id = %id, "progress saved")`). Libraries (`core`, `proto`) never install a subscriber. Never log passwords, tokens, `Authorization`/`Cookie` headers, or auth request bodies. Levels and log formats: see `CLAUDE.md`, [server.md](server.md) and [app.md](app.md#logging). Frontend code logs with `log` from `src/lib/log.ts`.
- **On-disk data** goes only in `<library>/.shelf/`; app config, caches and logs go in the app's private dirs. Aim to preserve unknown JSON fields when rewriting files; today only `config.json` does (see [known-issues.md](known-issues.md)).
- **Dependencies**: shared crate versions in the root `[workspace.dependencies]`, referenced with `{ workspace = true }`. (Some crates still pin their own: `shelf-proto`'s prost/pbjson, the Tauri crates, `rbook`, `fast_image_resize`.)
- **Frontend**: components import the backend from `@/api`; app-only behaviour branches on `IS_TAURI` / `IS_ANDROID`; full-screen surfaces use the `safe-area` utility. `tauri.conf.json` before-commands use the `{ "script", "cwd": "../frontend" }` form.
- **TypeScript 6**: no `baseUrl`.

## Recipes

### Add a library operation end to end

1. **Schema**: request/response messages in `proto/apipb/*.proto`; register any new file in `crates/proto/build.rs`. `buf lint`.
2. **Trait**: a method on `Library` in `crates/core/src/library/mod.rs`.
3. **Core**: if it touches books, folders, progress or the last book, implement it on `LibraryView` (`crates/core/src/library/view.rs`), respecting the scope (paths relative to its root, hidden books act as missing), and have `BookLibrary`'s trait method call `self.view(&EVERYTHING)`; otherwise implement it on `BookLibrary` (`book_library.rs`). Blocking file work through `with_storage` / `spawn_blocking`.
4. **App**: forward it in `LocalLibrary` (`libsrc/local.rs`), implement it with `api_call` in `RemoteLibrary` (`libsrc/remote.rs`), add a `dispatch!` arm in `AnyLibrary` (`libsrc/mod.rs`).
5. **Server**: a handler and route in `crates/server/src/api.rs`, calling `library.view(&user.scope)` (or a method forwarded in `ServerLibrary`, `crates/server/src/library.rs`, for one that doesn't depend on the scope).
6. **App command**: a `#[tauri::command]` in `app/src/commands.rs`, added to `generate_handler!` in `app/src/lib.rs`.
7. **Frontend**: add it to `Backend` (`api/types.ts`), `httpBackend` (`api/http.ts`), `tauriBackend` (`api/tauri.ts`) and the named exports in `api/index.ts`.

Keep the HTTP route and the command speaking the same proto JSON, and the Tauri argument named `req` (or `patch` for config) as the others are.

### Add an app-only native capability (Android)

1. A `@Command` method in `ShelfPlugin.kt` that resolves a `JSObject`.
2. A command in `app/src/native.rs` that calls it via `call::<T>(&app, "methodName")` on Android and returns a desktop fallback elsewhere; register it in `lib.rs`.
3. A wrapper in `frontend/src/api/app.ts`.

### Add an app-only setting

Render it behind `IS_TAURI` in the relevant section's `content` component (see `Libraries` in `components/settings/LibrarySection.tsx`), keeping its hooks in their own component so the web never calls Tauri commands. For a new section, add it to `SECTIONS` in `sections.ts` (extend the `slug` union); mark a section that's entirely app-only with `appOnly: true`, as *Developer options* is. A per-device setting is a field of `AppData` (`app/src/app_data.rs`, stored in `app_data.json`) changed through a Rust command, as the log-file count is (`set_log_files` in `app/src/logging/mod.rs`).

### Add a library-wide setting

Add an `optional` field to `proto/configpb/config.proto` (never reuse a number), then a `Setting` in `sections.ts` with `read`/`patch`, and use it via `resolveAppearance` or `useLibraryConfig`. `set_config` merges partial patches, so nothing on the Rust side changes.
