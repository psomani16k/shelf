# Known issues

Problems and risks in what's built, checked against the code on 2026-11-09. Planned work, including fixes for most of these, is in [roadmap.md](roadmap.md).

## Data safety

- **Progress is deleted with a missing book.** The startup sync removes every `.shelf/books/<id>/` folder that no file produced, `progress.json` included. A book that's temporarily absent at startup (a sync tool halfway through, an unmounted folder) loses its progress. A book file that exists but can't be read (a permission error) and editing a book's bytes, which changes its ID, have the same effect.
- **Unrecognised entries in `.shelf/books/` are deleted.** Every entry there is treated as a book ID, so a sync-tool conflict copy of a book folder is removed by the startup sync.
- **Unknown fields are kept only in `config.json`.** pbjson drops unknown fields on read, so rewriting `state.json` (`set_state`) or `metadata.json` (a moved book) loses fields from a newer build. `set_progress` writes a fresh `progress.json`, so it drops them too. The comment in `BookLibrary::set_state` says the opposite.
- **`last_update_time` limits** (it's the time this device last hashed the book, compared with the file's mtime):
  - `metadata.json` differs per device, since each writes its own clock. Two devices hashing the same book around the same time can leave sync-conflict copies in `.shelf/books/<id>/` (harmless: only `metadata.json` is read).
  - A file replaced at the same path by a different one whose mtime is *older* than the stamp (sync tools and `cp -p` preserve mtimes) isn't re-hashed, so its metadata and ID stay stale until the file's mtime moves forward again or the index is rebuilt (*Settings → Library → Rebuild index*).
  - A file with an mtime in the future is hashed on every start until the clock passes it.
  - Clock skew between devices shifts the comparison by the same amount.
- Two identical copies of a file share an ID and one metadata entry, so only one of them shows in listings.
- Metadata is only what the OPF declares: a book with no `dc:title` shows its file name and no authors, and an author written without the `dc:` prefix (`<creator xmlns="http://purl.org/dc/elements/1.1/">`) isn't read by rbook.

## Performance

- **The first start after upgrading hashes every book once**, because older `metadata.json` files have no `last_update_time`. Later starts only hash new and modified books.
- A watcher event for a path whose mtime moved (a touch, a sync tool rewriting the same bytes) re-hashes the whole file after the 5 s debounce. A file that keeps changing never syncs, since there's no upper bound on the debounce.
- A book deleted while the library is open makes its watcher job read every `metadata.json` to find its ID, so deleting a large folder (one event per book) reads the whole of `.shelf/books/` once per book.
- The startup sync runs two extraction jobs per hardware thread, each holding a whole book in memory. The watcher has its own cap of one per hardware thread, so a *Rebuild index* while the watcher is busy runs both at once.
- A folder moved, renamed or copied into the library is synced in one job, one book after another, and each of its books is read and hashed in full (the index has no entry for their new paths). A large folder takes a while to show up.
- **Linux app is laggy** and slow to show its first frame on the developer's machine (WebKitGTK on hybrid NVIDIA + AMD under Wayland/niri). Not caused by the backend. See [app.md](app.md#linux) for what was tried.

## Behaviour

- `Library::list_books` and `list_sub_directories` can't fail. A remote library that can't reach its server logs a `warn` and shows an empty folder instead of an error.
- **Settings can't be saved in a library with no books yet.** `.shelf/` is only created when the first book is extracted (`create_book_dir`), and `write_atomic` doesn't create it, so `set_config` fails with `couldn't write …/config.json`.
- A book whose file is gone but is still in the index (until the watcher's sync): the server's `/file` returns `ServeFile`'s empty 404, the app's `get_book_file` a 500.
- Folders with no books in them never appear in listings (folders are derived from book paths).
- A library can't be renamed or repointed; remove it and add it again (nothing in the library is lost).
- `RemoteLibrary`: the download cache is never evicted and isn't deleted when the library is removed; there's a 10 s connect timeout but no overall request timeout.
- The server has no authentication. Run it only on a trusted network: anyone who reaches it can read, change progress and settings, and start a *Rebuild index* (slow but harmless).
- The server's library path (`/books`) and bind address (`0.0.0.0:8080`) are hard-coded, and `client_ip` behind a proxy is the proxy.
- Reader zoom doesn't scale images: they get the inverse zoom so foliate's px height cap still fits them to the page. An image sized in px in the book's CSS stays that size at any zoom.
- The interface scale doesn't move breakpoints: media queries (`md:`, Settings' `48rem`) use the browser's default font size, not the root's. At 150% a narrow desktop window keeps the wide layout with bigger controls. Bits sized in px (the 3px position lines, borders) don't scale.

## Platform limits

- **Android folder picker** only maps folders on device storage (`com.android.externalstorage.documents`); other document providers are rejected.
- `MANAGE_EXTERNAL_STORAGE` is restricted on the Play Store. Fine for sideloading, which is the plan. Play Protect may still flag the sideloaded APK; *Install anyway* or `adb install` gets past it, and Google's free hobbyist developer account (up to 20 devices) registers the app.
- **Windows (not a target):** `metadata.path` comes from `Path::to_string_lossy`, so on Windows it would hold `\` separators, breaking folder listings (which split on `/`) and libraries shared with other devices. Normalize to `/` before any Windows build.

## Cleanups

- `shelf-core` has pre-existing clippy warnings.
- `core` uses blocking `std::fs` (through `spawn_blocking`), against the "tokio everywhere" rule.
- Unused frontend code and deps: `bookTitle` in `src/lib/book.ts`, `src/components/ui/sheet.tsx`, the npm packages `@tauri-apps/plugin-http` and `@tauri-apps/plugin-store` (the app uses both plugins from Rust only).
- Unused proto: the `FolderListing` message in `bookkeeperpb/metadata.proto`; `CreateDirectoryRequest`, `CreateDirectoryResponse` and `UploadEbookRequest` in `apipb/library.proto`; `DeveloperConfig.logging` in `configpb/config.proto` (the app keeps its log-file count in `AppData`); `apipb/cover.proto` is empty (and not in `build.rs`).
- `crates/core/src/library_old.rs` isn't declared in `lib.rs`, so it never compiles. `crates/demo/temp.json` is a committed log dump.
- The demo's `debounce` never returns: it calls `tx.closed()` without awaiting it and never drops `tx`, so the `Debounced` task waits forever.
- `app/Cargo.toml` still has template metadata (`description = "A Tauri App"`, `authors = ["you"]`).
- `frontend/README.md` is the Vite template's README.

## S3 storage

`S3LibraryStorage` (`crates/core/src/storage/s3_storage.rs`) isn't used yet ([roadmap](roadmap.md#library-core)). Once it is:

- **No watcher.** Object stores send no change events, so books added or removed in the bucket only show up at the next startup sync or *Rebuild index*.
- **Startup costs requests per book:** one HEAD for its mtime and one GET for its `metadata.json`, on top of the LIST calls. A book that has to be hashed is downloaded whole.
- **Covers and books are buffered in memory** (`FileContent::Bytes`): the server sends them whole, with no Range requests. A `FileContent::Stream` variant fed into axum's `Body::from_stream` would fix that for the server; the app reads whole books anyway.
