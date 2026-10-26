# Development

## Setup

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
| Web UI against a local server | `cargo run -p shelf-server` (library at `/books`), then `pnpm --dir frontend dev` → http://localhost:5173 |
| Linux app | `cd app; and cargo tauri dev` |
| Android app on a USB phone | `cd app; and cargo tauri android dev` |
| Android release APK | `cd app; and cargo tauri android build --apk --target aarch64` (see [app.md](app.md)) |
| Server image | `docker build -t ghcr.io/psomani16k/shelf:<version> -t ghcr.io/psomani16k/shelf:latest .` (see [server.md](server.md)) |

## Checks

```fish
cargo fmt; and cargo clippy --workspace -- -D warnings; and cargo test --workspace
buf lint
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend exec tsc -b
```

`shelf-core` has pre-existing clippy warnings, so `--workspace -D warnings` fails on them. To lint just the app: `cargo clippy -p app --no-deps -- -D warnings`.

Rust tests exist in `crates/core/src/config.rs` and `crates/core/src/state.rs` (tokio tests with `tempfile`). There are none yet for the server or the app.

## Working with this user

- They're a backend engineer (mostly Go) who knows Rust outside web work, and is using the project to learn frontend/app development and Rust servers. Claude writes the code, explaining the *why* briefly, with Go comparisons where they help.
- **They run the Rust checks and builds themselves.** Don't run `cargo fmt`, `cargo clippy`, `cargo build`/`test` or Docker builds unless asked; say which command to run instead. Frontend checks (`tsc`, oxlint, vitest, vite build) are fine to run.
- **Edit only the files a request names.** Suggest wider refactors in chat instead of making them.
- Do chores (config, docs, mechanical fixes) directly.
- Few code comments: only a non-obvious "why" or a trap, in one line.
- Don't edit `frontend/vendor/foliate-js`, `frontend/src/gen/`, or generated Rust in `target/`.
- Never print or commit the Android keystore passwords (`app/gen/android/keystore.properties`).

## Code conventions

- **tokio everywhere**: async APIs, `tokio::spawn`, `tokio::sync`, `tokio::time`, `tokio::fs`. Blocking or CPU-heavy work (EPUB/ZIP parsing, hashing, image resizing) goes in `spawn_blocking`. Timing tests use `#[tokio::test(start_paused = true)]`.
- **Errors**: `anyhow` inside, `LibraryError` at the `Library` boundary, logged once where they leave Rust (`crates/server/src/error.rs`, `app/src/error.rs`).
- **Logging**: `tracing` with structured fields (`info!(book_id = %id, "progress saved")`). Libraries (`core`, `proto`) never install a subscriber. Never log passwords, tokens, `Authorization`/`Cookie` headers, or auth request bodies. Levels and server log format: see `CLAUDE.md` and [server.md](server.md).
- **On-disk data** goes only in `<library>/.shelf/`; app config, caches and logs go in the app's private dirs. Aim to preserve unknown JSON fields when rewriting files; today only `config.json` does (see [known-issues.md](known-issues.md)).
- **Dependencies**: shared crate versions in the root `[workspace.dependencies]`, referenced with `{ workspace = true }`. (Some crates still pin their own: `shelf-proto`'s prost/pbjson, the Tauri crates, `rbook`, `fast_image_resize`.)
- **Frontend**: components import the backend from `@/api`; app-only behaviour branches on `IS_TAURI` / `IS_ANDROID`; full-screen surfaces use the `safe-area` utility. `tauri.conf.json` before-commands use the `{ "script", "cwd": "../frontend" }` form.
- **TypeScript 6**: no `baseUrl`.

## Recipes

### Add a library operation end to end

1. **Schema**: request/response messages in `proto/apipb/*.proto`; register any new file in `crates/proto/build.rs`. `buf lint`.
2. **Trait**: a method on `Library` in `crates/core/src/traits.rs`.
3. **Core**: implement it on `BookLibrary` (`crates/core/src/library.rs`). Blocking file work through `with_keeper` / `spawn_blocking`.
4. **App**: forward it in `LocalLibrary` (`libsrc/local.rs`), implement it with `api_call` in `RemoteLibrary` (`libsrc/remote.rs`), add a `dispatch!` arm in `AnyLibrary` (`libsrc/mod.rs`).
5. **Server**: a handler and route in `crates/server/src/api.rs`.
6. **App command**: a `#[tauri::command]` in `app/src/commands.rs`, added to `generate_handler!` in `app/src/lib.rs`.
7. **Frontend**: add it to `Backend` (`api/types.ts`), `httpBackend` (`api/http.ts`), `tauriBackend` (`api/tauri.ts`) and the named exports in `api/index.ts`.

Keep the HTTP route and the command speaking the same proto JSON, and the Tauri argument named `req` (or `patch` for config) as the others are.

### Add an app-only native capability (Android)

1. A `@Command` method in `ShelfPlugin.kt` that resolves a `JSObject`.
2. A command in `app/src/native.rs` that calls it via `call::<T>(&app, "methodName")` on Android and returns a desktop fallback elsewhere; register it in `lib.rs`.
3. A wrapper in `frontend/src/api/app.ts`.

### Add an app-only setting

Add a `Section` with a `content` component to `APP_SECTIONS` in `frontend/src/components/settings/sections.ts` (extend the `slug` union).

### Add a library-wide setting

Add an `optional` field to `proto/configpb/config.proto` (never reuse a number), then a `Setting` in `sections.ts` with `read`/`patch`, and use it via `resolveAppearance` or `useLibraryConfig`. `set_config` merges partial patches, so nothing on the Rust side changes.
