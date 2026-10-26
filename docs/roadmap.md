# Roadmap

Everything planned but not built, as of 2026-10-26. It comes from the old `PLAN.md` (removed in commit `aed1ede`) and the earlier `CLAUDE.md` design, checked against the code. Bugs and risks in what *is* built are in [known-issues.md](known-issues.md); the reasoning behind the design is in [decisions.md](decisions.md).

Rough priority within each section is top to bottom. Plan milestones are noted as (M*n*) where they came from one.

## Authentication

From the plan's server design (M4, M5). Nothing here exists; today the server is open to anyone who can reach it.

- [ ] Users in the server config: `[[users]] name`, `password_hash` (argon2id).
- [ ] `shelf-server hash-password`: prompts and prints an argon2id hash.
- [ ] `POST /api/auth/login {username, password}`: checks the hash, creates a random 32-byte token, stores only `sha256(token)` with user and expiry (90 days, sliding) in `state_dir/sessions.json`. Responds with `{ token }` and sets an `HttpOnly; Secure; SameSite=Lax` cookie.
- [ ] Requests authenticate with the cookie (browser) or `Authorization: Bearer <token>` (app). Every `/api/*` route except `login` and `health` requires it.
- [ ] `POST /api/auth/logout` revokes the token; `GET /api/auth/me` returns the username or 401.
- [ ] In-memory per-IP rate limit on login attempts (log at `warn` when hit).
- [ ] Access log gets a `user` field; login success/failure logged at `info` with username and client IP. Never log passwords, tokens or their hashes, or auth request bodies.
- [ ] Web: a Login screen, shown on 401.
- [ ] App: login when adding a remote library; token kept in the app store (plaintext acceptable at first, OS keyring later); `RemoteLibrary` sends it as a bearer token; an expired or invalid token goes back to login.

## Server

- [ ] Config file and CLI (`shelf-server serve --config /etc/shelf/config.toml`) with `library`, `bind`, `state_dir` (sessions; never inside the library), `log_bodies`, trusted proxies. Today `/books` and `0.0.0.0:8080` are hard-coded.
- [ ] Trust `X-Forwarded-For` only from proxies listed in the config, for `client_ip` behind Caddy or Tailscale.
- [ ] `log_bodies = true`: access lines include JSON request and response bodies, cut at about 4 KB. Never for `/api/auth/*` or the raw-byte routes (`/file`, `/cover`).
- [ ] Integration tests against a temp library: every route, Range requests, progress round-trips, and auth flows and 401s once auth exists (M4).
- [ ] Deployment notes for HTTPS: Tailscale (`tailscale serve`) or Caddy in front of the container (M4/M7).

## Library core

- [x] **Faster startup:** done with `last_update_time` vs the file's mtime ([library-format.md](library-format.md#startup-sync)). Possible follow-up: also record the file's `size`, to catch a same-path replacement with an older mtime.
- [ ] **Watcher flush cap:** flush a path at the latest 60 s after its first event, so a file that keeps changing still syncs.
- [x] **Watcher modify filtering:** events that don't move the file's mtime (attribute or access changes) no longer re-hash it: `sync_path` checks `unchanged_id` first. A touch still does. (The TODO in `watcher.rs` about filtering event kinds is still there.)
- [ ] **Atomic writes** for `metadata.json`, `progress.json` and thumbnails (`write_atomic`).
- [ ] **Keep unknown fields** when rewriting `state.json`, `metadata.json` and `progress.json` (merge into raw JSON as `config.json` does).
- [ ] **Don't delete unrecognised entries in `.shelf/books/`**, such as sync-tool conflict copies; only remove folders that are valid IDs for books that are gone.
- [ ] **Keep progress across a book being briefly missing or edited** (e.g. don't delete a book's folder on the first startup it's missing; or derive the ID from the OPF `unique-identifier`, falling back to the file hash, as the plan proposed).
- [ ] Write a zero progress file at extraction (TODO in `ebook_ingest.rs`).
- [ ] Make `list_books` / `list_sub_directories` fallible in the `Library` trait, so a remote library's network errors show as errors, not an empty folder.
- [ ] Move rayon work onto tokio (`spawn_blocking`), per the "tokio everywhere" rule.
- [ ] Fixture-based tests with 2–3 small public-domain EPUBs (one without a cover): extraction, ID stability across renames, progress round-trips, the watcher with `start_paused` (M1).

## App

- [x] **Logging in release builds too**, as JSON lines like the server's, into one file per app start (the number kept is a setting) and stdout/logcat ([app.md](app.md#logging)).
- [x] **Settings → Logs** (now *Developer options*): log-file count, *Copy logs*, *Save logs…*.
- [x] **Frontend logging**: `src/lib/log.ts` and the `error`/`unhandledrejection` handlers ([frontend.md](frontend.md#logging-liblogts)).
- [x] **Multiple libraries** in the UI: named libraries, a switcher in the library slideover, add/remove in Settings ([frontend.md](frontend.md#libraries-libuseapplibraryts)).
- [ ] **Android back button**: close the open drawer or settings first. Today the app uses a `MemoryRouter`, so the WebView has no history and back exits the app.
- [ ] **Offline reading in Remote mode** (M9): books and covers are already cached after download; still needed are offline listings and queued progress writes.
- [ ] Remote cache housekeeping: evict old files, delete the cache when a library is removed, an overall request timeout.
- [ ] Device name setting (default hostname, or "android"), used for the progress `device` field below.

## Reading and library UI

- [ ] **Richer progress** (M2/M3): store `fraction`, `chapter` and `updated`/`device` alongside the CFI, so the library grid can show a progress bar per book without opening it, and the reader can show "Resumed where you left off on *phone* · 3 min ago" on open. (No mid-read prompts: one device at a time.)
- [ ] **Library search** by title or author; **sort** by recently read, title or author; a refresh button or pull-to-refresh.
- [ ] **Book details**: language, description, publisher, published date (extracted to `metadata.json`, a details view).
- [ ] **Library info**: optional `.shelf/library.json` (`{ "name": … }`) and an "About / library info" section in Settings (name, book count).
- [ ] **Reader**: chapter name and percentage display; an auto-hiding top bar; tap zones to turn pages; PageUp/PageDown/Space.

## Theming and reading customization

Built today: interface theme (system/light/dark), reader theme (match interface/light/dark), layout (scrolled/paginated) and four margin sizes, all in the library's `config.json` (`proto/configpb/config.proto`, resolved in `frontend/src/lib/appearance.ts`).

- [ ] **Decide where each preference lives** before adding more. Colours and layout suit the library config (the same on every device); size-like ones (font size, line height, margins) differ between a phone and a laptop and probably belong per device (app store / `localStorage`). Record the choice in [decisions.md](decisions.md).
- [ ] **More reader themes**: sepia, a low-contrast warm "night" theme, and pure black for OLED screens. New `ReaderTheme` enum values (append, never renumber), with `PALETTE` in `appearance.ts` becoming a table of themes and `readerCss` handling each.
- [ ] **Typography**:
  - font size, in steps;
  - font family: the book's own, serif, sans-serif, and a dyslexia-friendly option, bundled with the frontend, since Android WebViews ship few fonts;
  - line height and paragraph spacing;
  - justification and hyphenation;
  - a *Use publisher's styles* toggle: off overrides the book's fonts and spacing, on leaves them alone.
- [ ] **Quick settings in the reader**: an "Aa" panel over the book for theme, font and size, with a live preview, so changing them doesn't mean leaving the book for Settings.
- [ ] **Dark-theme images**: an option to dim images in dark themes, so a white illustration doesn't flash.
- [ ] **Interface accent colour**, and an interface theme that follows the reader theme (so sepia reading gets a matching drawer and settings).
- [ ] App only: **keep the screen on** while reading.
- [ ] Later: custom CSS per library for anything the presets don't cover.

## Packaging and project

- [ ] Linux packages: AppImage and `.deb` via `cargo tauri build` (bundle targets are `all` but this hasn't been tried) (M7).
- [ ] A top-level `README.md` with setup for all three modes (M7).
- [ ] Add `buf breaking` against the last release to the checks whenever `.proto` files change (`buf.yaml` already configures `FILE` rules).
- [ ] A local check script running fmt, clippy, tests, `buf lint` and the frontend checks (no CI/CD for now; images are pushed by hand).
- [ ] Optional end-to-end smoke test with Playwright against the web build: open a book, turn a page, reload, check the position.
- [ ] Decide on the final app name before a wider release (see [decisions.md](decisions.md#risks)).

## Later

From the plan's "not v1" list, in rough priority order:

- **Annotations** (M8): bookmarks and highlights in `.shelf/books/<id>/annotations.json`.
- **More formats** (M10): MOBI/AZW3, FB2 and CBZ via foliate-js, with metadata support in `core`.
- Full-text search across the library.
- Collections and tags.
- Reading stats.
- Android Storage Access Framework support, only if Play Store distribution ever matters.
