# Roadmap

Everything planned but not built, as of 2026-11-10. It comes from the old `PLAN.md` (removed in commit `aed1ede`) and the earlier `CLAUDE.md` design, checked against the code. Bugs and risks in what *is* built are in [known-issues.md](known-issues.md); the reasoning behind the design is in [decisions.md](decisions.md).

Rough priority within each section is top to bottom. Plan milestones are noted as (M*n*) where they came from one.

## Users and authentication

From the plan's server design (M4, M5), extended to per-user progress (design in [decisions.md](decisions.md#users-and-shelf-partly-built)).

### Per-user data in `.shelf/` (core)

Built: progress, the last book and settings are per user ([library-format.md](library-format.md)), with the user in the request's proto message, filled in by the server's handlers (the signed-in user) and by `LocalLibrary` (the library entry's `user`, `default` for now). Old `progress.json`, `config.json` and `state.json` files are ignored (a deliberate breaking change). Book metadata and thumbnails stay shared.

- [ ] `users/<user>/` folders (settings and last book) are never deleted automatically.

### Server auth

Built: login with bearer tokens, sessions in `/data/data.json` (90 days, sliding), logout, `me`, password change, roles enforced per route, the per-username lockout, and `user` in the access log ([api.md](api.md#http-routes-cratesserversrcapirs), [server.md](server.md#server-data)). The web and the app's remote libraries have a login screen, stay signed in, and hide what the role can't do; *Settings → Account* shows the user, changes the password and logs out ([frontend.md](frontend.md)). Sign-up from the login screen, approved or denied by an admin in *Settings → Admin*, where admins also change other users' roles and ban them ([api.md](api.md#http-routes-cratesserversrcapirs)).

- [x] **Library root and book access per user** (`library_root` and `books` in `data.json`), with identical files as one book with several copies ([server.md](server.md#server-data), [api.md](api.md#http-routes-cratesserversrcapirs)).
- [x] An admin UI for each user's `library_root` and `books`, with `may_manage` for who may change whose ([api.md](api.md#http-routes-cratesserversrcapirs), [frontend.md](frontend.md)).
- [x] Frontend: delete one copy with `DeleteItemsRequest.books`, and key and select listing items by file name, so two copies in one folder are separate ([frontend.md](frontend.md)).
- [x] Making users through the API: approving sign-ups (`ADMIN` for roles below admin, `OWNER` for admins).
- [x] Changing a user's role and banning through the API (`ADMIN` for users below admin, `OWNER` for admins too, per [api.md](api.md#access-by-role)).
- [ ] Removing users through the API, with the same rule.
- [ ] App: keep a remote library's token in the OS keyring instead of `app_data.json`.

### App, local libraries

- [ ] Ask for the username to use (stored as the entry's `user` in `AppData` and used by `LocalLibrary` already, always `default` today) when adding it, from the names that already have progress or typed in. No password and no role: every feature is available. Existing entries get asked once on open.
- [ ] Show the current user in the library slideover too; changing a local library's user is allowed (it's not a security boundary).

## Server

- [ ] Config file and CLI (`shelf-server serve --config /etc/shelf/config.toml`) with `library`, `bind`, the data folder (today `/data`; never inside the library), `log_bodies`. Today `/books` and `0.0.0.0:8080` are hard-coded.
- [ ] `log_bodies = true`: access lines include JSON request and response bodies, cut at about 4 KB. Never for `/api/auth/*` or the raw-byte routes (`/file`, `/cover`).
- [ ] Integration tests against a temp library: every route, Range requests, progress round-trips (M4). The auth flows, 401s and 403s are covered by `api.rs`'s router tests.
- [ ] Deployment notes for HTTPS: Tailscale (`tailscale serve`) or Caddy in front of the container (M4/M7).

## Library core

- [x] **Faster startup:** done with `last_update_time` vs the file's mtime ([library-format.md](library-format.md#startup-sync)). Possible follow-up: also record the file's `size`, to catch a same-path replacement with an older mtime.
- [ ] **Watcher flush cap:** flush a path at the latest 60 s after its first event, so a file that keeps changing still syncs.
- [x] **Watcher modify filtering:** events that don't move the file's mtime (attribute or access changes) no longer re-hash it: the watcher's sync job (`BookLibrary::receive_and_process_library_updates`) checks `predict_id_change_using_modified_time` first. A touch still does.
- [x] **Don't lose progress to a corrupt `metadata.json`:** `EbookExtractor::read` treats one it can't read as missing (with a `warn`), so the book is extracted again and its folder kept ([library-format.md](library-format.md#startup-sync)).
- [x] **Atomic writes** for `metadata.json`, `progress.json` and thumbnails too ([library-format.md](library-format.md#writes)). Together with the item above, a crash during *Rebuild index* can't cost a book its progress.
- [x] **Keep progress across a book being missing:** the startup sync deletes only a gone book's `metadata.json` and thumbnail, so its progress is there when the same file comes back ([library-format.md](library-format.md#startup-sync)).
- [ ] **Keep progress across a book being edited:** e.g. derive the ID from the OPF `unique-identifier`, falling back to the file hash, as the plan proposed.
- [x] A way to delete the kept progress of books that are gone for good: *Settings → Library → Clean up progress* ([library-format.md](library-format.md#cleaning-up-progress)).
- [x] **Watcher: a gone book costs no `metadata.json` reads.** `IndexSync::sync_book` drops a gone file's entries by path before calling `EbookExtractor::read` ([library-format.md](library-format.md#watcher)).
- [ ] **Clean up leftover upload temp files** (`.<name>.part-*` next to a book, left by a crash mid-upload) when a library opens, as `.shelf/` temp files are.
- [ ] **Stream uploads to disk** instead of holding the whole book in memory, on the server and in the app.
- [ ] **Extraction concurrency:** measure the startup sync's two jobs per CPU against one (32 vs 16 on a 16-thread machine, and up to that many books in memory) and keep whichever is faster. Then share one cap (a `Semaphore` in `BookLibrary`) between the startup sync, *Rebuild index* and the watcher, which today each cap themselves.
- [ ] **Wire up S3 libraries.** `S3LibraryStorage` is written and tested but unused. Left: a dispatch enum over the storage backends (the trait returns `impl Future`, so no `dyn`), used by `BookLibrary`, `EbookExtractor`, `IndexSync` and `sync_library`; `BookLibrary::open` skipping the watcher for S3; and configuration (server env var, maybe an app library kind).
- [ ] Make `list_books` / `list_sub_directories` fallible in the `Library` trait, so a remote library's network errors show as errors, not an empty folder.
- [x] Move rayon work onto tokio (`spawn_blocking`), per the "tokio everywhere" rule. Done: the startup sync uses `shelf_utility::routines::transform_concurrent_blocking`, and rayon is out of the workspace.
- [ ] Fixture-based tests with 2–3 small public-domain EPUBs (one without a cover): extraction, ID stability across renames, progress round-trips, the watcher with `start_paused` (M1). Also each step of `find_cover` (a declared cover, the `cover` item's page, a `<meta>` holding a path, the landmarks page) and a cover that can't be decoded; tiny EPUBs built in memory with the `zip` crate as a dev-dependency would do.
- [ ] Report to rbook: a `<creator xmlns="http://purl.org/dc/elements/1.1/">` (the Dublin Core namespace without the `dc:` prefix) isn't read as an author (seen in *PoC or GTFO, Volume 2*).

## App

- [x] **Logging in release builds too**, as JSON lines like the server's, into one file per app start (the number kept is a setting) and stdout/logcat ([app.md](app.md#logging)).
- [x] **Settings → Logs** (now *Developer options*): log-file count, *Copy logs*, *Save logs…*.
- [x] **Frontend logging**: `src/lib/log.ts` and the `error`/`unhandledrejection` handlers ([frontend.md](frontend.md#logging-liblogts)).
- [x] **Multiple libraries** in the UI: named libraries, a switcher in the library slideover, add/remove in Settings ([frontend.md](frontend.md#libraries-libuseapplibraryts)).
- [x] **Android back button**: closes the open drawer or settings first ([frontend.md](frontend.md#android-back-button-libbackbuttonts)).
- [ ] **Offline reading in Remote mode** (M9): books and covers are already cached after download; still needed are offline listings and queued progress writes.
- [ ] Remote cache housekeeping: evict old files, delete the cache when a library is removed, an overall request timeout.
- [ ] Device name setting (default hostname, or "android"), used for the progress `device` field below.

## Reading and library UI

- [ ] **Richer progress** (M2/M3): store `fraction`, `chapter` and `updated`/`device` alongside the CFI, so the library grid can show a progress bar per book without opening it, and the reader can show "Resumed where you left off on *phone* · 3 min ago" on open. (No mid-read prompts: one device at a time.)
- [ ] **Library search** by title or author; **sort** by recently read, title or author; a refresh button or pull-to-refresh.
- [ ] **Book details**: language, description, publisher, published date (extracted to `metadata.json`, a details view).
- [ ] **Library info**: optional `.shelf/library.json` (`{ "name": … }`) and an "About / library info" section in Settings (name, book count).
- [x] **Reader bars**: chapter name and percentage in a bottom bar; the header and bottom bar auto-hide while reading ([frontend.md](frontend.md#home-and-reading)).
- [ ] **Reader input**: tap zones to turn pages; PageUp/PageDown/Space turning pages when paginated (today they only hide the bars).

## Theming and reading customization

Built today: interface theme (system/light/dark), reader theme (match interface/light/dark), layout (scrolled/paginated) and four margin sizes, all per user in the library's `users/<user>/config.json` (`proto/configpb/config.proto`, resolved in `frontend/src/lib/appearance.ts`). Reader zoom is per device, in `localStorage` (`frontend/src/lib/readerZoom.ts`). The app's interface scale is per device, in the app data (`frontend/src/lib/uiScale.ts`), and the book's zoom is multiplied by it.

- [ ] **Decide where each preference lives** before adding more. Settings are per user now, the same on all of that user's devices; size-like ones (font size, line height, margins) differ between a phone and a laptop and might belong per device instead (app store / `localStorage`), as zoom and the interface scale do. Record the choice in [decisions.md](decisions.md).
- [ ] **More reader themes**: sepia, a low-contrast warm "night" theme, and pure black for OLED screens. New `ReaderTheme` enum values (append, never renumber), with `PALETTE` in `appearance.ts` becoming a table of themes and `readerCss` handling each.
- [ ] **Typography**:
  - font size, in steps;
  - font family: the book's own, serif, sans-serif, and a dyslexia-friendly option, bundled with the frontend, since Android WebViews ship few fonts;
  - line height and paragraph spacing;
  - justification and hyphenation;
  - a *Use publisher's styles* toggle: off overrides the book's fonts and spacing, on leaves them alone.
- [ ] **Quick settings in the reader**: an "Aa" panel over the book for theme, font and size, with a live preview, so changing them doesn't mean leaving the book for Settings. Started: the bottom bar's *Aa* popup has zoom (per device, `lib/readerZoom.ts`).
- [ ] **Dark-theme images**: an option to dim images in dark themes, so a white illustration doesn't flash.
- [ ] **Interface accent colour**, and an interface theme that follows the reader theme (so sepia reading gets a matching drawer and settings).
- [ ] App only: **keep the screen on** while reading.
- [ ] Later: custom CSS per library for anything the presets don't cover.

## Packaging and project

- [ ] Linux packages: AppImage and `.deb` via `cargo tauri build` (bundle targets are `all` but this hasn't been tried) (M7).
- [x] A top-level `README.md` (what Shelf is, running the server, building the app) and `CONTRIBUTING.md` (getting started as a contributor) (M7).
- [ ] Add `buf breaking` against the last release to the checks whenever `.proto` files change (`buf.yaml` already configures `FILE` rules).
- [ ] A local check script running fmt, clippy, tests, `buf lint` and the frontend checks (no CI/CD for now; images are pushed by hand).
- [x] Document the `demo` crate (`crates/demo`): what it's for, its functions and a `jq` filter for its output ([development.md](development.md#the-demo-crate)).
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
