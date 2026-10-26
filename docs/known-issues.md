# Known issues

Problems and risks in what's built, checked against the code on 2026-10-26. Planned work, including fixes for most of these, is in [roadmap.md](roadmap.md).

## Data safety

- **Progress is deleted with a missing book.** The startup sync removes every `.shelf/books/<id>/` folder that no file produced, `progress.json` included. A book that's temporarily absent at startup (a sync tool halfway through, an unmounted folder) loses its progress. Editing a book's bytes changes its ID, which has the same effect.
- **Unrecognised entries in `.shelf/books/` are deleted.** Every entry there is treated as a book ID, so a sync-tool conflict copy of a book folder is removed by the startup sync.
- **Not all writes are atomic.** `state.json` and `config.json` use `write_atomic`; `metadata.json`, `progress.json` and `thumbnail.jpeg` use plain `fs::write` (`crates/core/src/bookkeeper.rs`). A crash or a sync tool can see a half-written progress file; a corrupt one is read as "no progress" with a warning.
- **Unknown fields are kept only in `config.json`.** pbjson drops unknown fields on read, so rewriting `state.json` (`set_state`) or `metadata.json` (a moved book) loses fields from a newer build. The comment in `BookLibrary::set_state` says the opposite.
- **`last_update_time` limits** (it's the time this device last hashed the book, compared with the file's mtime):
  - `metadata.json` differs per device, since each writes its own clock. Two devices hashing the same book around the same time can leave sync-conflict copies in `.shelf/books/<id>/` (harmless: only `metadata.json` is read).
  - A file replaced at the same path by a different one whose mtime is *older* than the stamp (sync tools and `cp -p` preserve mtimes) isn't re-hashed, so its metadata and ID stay stale until the file's mtime moves forward again or the index is rebuilt (*Settings → Library → Rebuild index*).
  - A file with an mtime in the future is hashed on every start until the clock passes it.
  - Clock skew between devices shifts the comparison by the same amount.
- Two identical copies of a file share an ID and one metadata entry, so only one of them shows in listings.

## Performance

- **The first start after upgrading hashes every book once**, because older `metadata.json` files have no `last_update_time`. Later starts only hash new and modified books.
- A watcher event for a path whose mtime moved (a touch, a sync tool rewriting the same bytes) re-hashes the whole file after the 5 s debounce. A file that keeps changing never syncs, since there's no upper bound on the debounce.
- **Linux app is laggy** and slow to show its first frame on the developer's machine (WebKitGTK on hybrid NVIDIA + AMD under Wayland/niri). Not caused by the backend. See [app.md](app.md#linux) for what was tried.

## Behaviour

- `Library::list_books` and `list_sub_directories` can't fail. A remote library that can't reach its server logs a `warn` and shows an empty folder instead of an error.
- Folders with no books in them never appear in listings (folders are derived from book paths).
- A library can't be renamed or repointed; remove it and add it again (nothing in the library is lost).
- `RemoteLibrary`: the download cache is never evicted and isn't deleted when the library is removed; there's a 10 s connect timeout but no overall request timeout.
- The app has no logging in release builds; debug builds log plain text, not JSON.
- On Android the back button exits the app even with a drawer or settings open (the `MemoryRouter` leaves the WebView without history).
- The server has no authentication. Run it only on a trusted network: anyone who reaches it can read, change progress and settings, and start a *Rebuild index* (slow but harmless).
- The server's library path (`/books`) and bind address (`0.0.0.0:8080`) are hard-coded, and `client_ip` behind a proxy is the proxy.

## Platform limits

- **Android folder picker** only maps folders on device storage (`com.android.externalstorage.documents`); other document providers are rejected.
- `MANAGE_EXTERNAL_STORAGE` is restricted on the Play Store. Fine for sideloading, which is the plan. Play Protect may still flag the sideloaded APK; *Install anyway* or `adb install` gets past it, and Google's free hobbyist developer account (up to 20 devices) registers the app.
- **Windows (not a target):** `metadata.path` comes from `Path::to_string_lossy`, so on Windows it would hold `\` separators, breaking folder listings (which split on `/`) and libraries shared with other devices. Normalize to `/` before any Windows build.

## Cleanups

- `shelf-core` has pre-existing clippy warnings, and an unused `use serde::{Deserialize, Serialize}` in `library.rs`.
- `core` uses rayon and blocking `std::fs` (through `spawn_blocking`), against the "tokio everywhere" rule.
- Unused frontend code and deps: `bookTitle` in `src/lib/book.ts`, `src/components/ui/sheet.tsx`, the npm packages `@tauri-apps/plugin-http` and `@tauri-apps/plugin-store` (the app uses both plugins from Rust only).
- Unused proto: the `FolderListing` message in `bookkeeperpb/metadata.proto`; `apipb/cover.proto` is empty (and not in `build.rs`).
- `app/Cargo.toml` still has template metadata (`description = "A Tauri App"`, `authors = ["you"]`).
- `frontend/README.md` is the Vite template's README.
