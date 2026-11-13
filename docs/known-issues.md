# Known issues

Problems and risks in what's built, checked against the code on 2026-11-13. Planned work, including fixes for most of these, is in [roadmap.md](roadmap.md).

## Data safety

- **Progress of a removed book is kept until someone cleans it up.** The startup sync deletes only the `metadata.json` and thumbnail of a book no file produced, so a book that comes back (same bytes) resumes; its folder and progress stay until an admin runs *Settings → Library → Clean up progress*. Running it while a book is only temporarily missing, after a startup that didn't find it, deletes that book's progress for good. Empty `.shelf/books/<id>/` folders also stay until then. A book that's temporarily absent at startup (a sync tool halfway through, an unmounted folder) is extracted again when it's back. Editing a book's bytes changes its ID, so the edited book starts with no progress; the old ID's progress stays.
- **`last_update_time` limits** (each copy's stamp is the time this device last hashed it, compared with the file's mtime):
  - `metadata.json` differs per device, since each writes its own clock. Two devices hashing the same book around the same time can leave sync-conflict copies in `.shelf/books/<id>/` (harmless: only `metadata.json` is read).
  - A file replaced at the same path by a different one whose mtime is *older* than the stamp (sync tools and `cp -p` preserve mtimes) isn't re-hashed, so its metadata and ID stay stale until the file's mtime moves forward again or the index is rebuilt (*Settings → Library → Rebuild index*).
  - A file with an mtime in the future is hashed on every start until the clock passes it.
  - Clock skew between devices shifts the comparison by the same amount.
- Metadata is only what the OPF declares: a book with no `dc:title` shows its file name and no authors, and an author written without the `dc:` prefix (`<creator xmlns="http://purl.org/dc/elements/1.1/">`) isn't read by rbook.

## Performance

- **The first start after upgrading hashes every book once**, because older `metadata.json` files have no `locations`. Later starts only hash new and modified copies.
- A watcher event for a path whose mtime moved (a touch, a sync tool rewriting the same bytes) re-hashes the whole file after the 5 s debounce. A file that keeps changing never syncs, since there's no upper bound on the debounce.
- **Uploads are held in memory whole:** on the server (up to 512 MiB per request), and in the app, which reads the assembled file back before handing it to the library. Uploads run one at a time in the frontend.
- The startup sync runs two extraction jobs per hardware thread, each holding a whole book in memory. The watcher has its own cap of one per hardware thread, so a *Rebuild index* while the watcher is busy runs both at once.
- A folder moved, renamed or copied into the library is synced in one job, one book after another, and each of its books is read and hashed in full (the index has no entry for their new paths). A large folder takes a while to show up.
- **Linux app is laggy** and slow to show its first frame on the developer's machine (WebKitGTK on hybrid NVIDIA + AMD under Wayland/niri). Not caused by the backend. See [app.md](app.md#linux) for what was tried.

## Behaviour

- `Library::list_books` and `list_sub_directories` can't fail. A remote library that can't reach its server logs a `warn` and shows an empty folder instead of an error.
- A book whose file is gone but is still in the index (until the watcher's sync): the server's `/file` returns `ServeFile`'s empty 404, the app's `get_book_file` a 500.
- A crash or a closed app mid-upload can leave a hidden `.<name>.part-<pid>-<seq>` file next to where the book was going (local libraries and the server). Nothing removes it yet. The app's own `uploads/` cache is cleared on start.
- Copies are only recognised byte for byte (same SHA-1). The same book from another source is a second book, with its own progress.
- **Folder visibility follows the index:** with `books` patterns, a folder is hidden when the books the index knows under it are all hidden. A book copied in from outside Shelf counts only once the watcher has synced it (5 s), and one that fails extraction never counts. Folder deletes check the disk instead, so they never delete a hidden book.
- **A scoped user can tell a hidden folder is there** when creating a folder with its name: theirs becomes `name (1)`. Uploads don't have this problem: hidden names are skipped whether or not they're taken.
- **A bad `library_root` or `books` pattern in `data.json`** keeps the last good copy in use, but every server write (logins, logouts, session extensions) fails until it's fixed, because writes re-read the file first.
- After a copy is deleted or moved, its book's `metadata.json` may still list the old path until the book's next write or the next startup. Only the in-memory index is used meanwhile, so nothing shows it.
- An upload or delete through the API changes the index at once; one done outside Shelf still waits for the watcher's 5 s debounce.
- **Android file chooser:** uploads use a plain `<input type="file">`, which relies on Tauri's WebView opening the system picker. Not yet tried on the phone.
- A library can't be renamed or repointed; remove it and add it again (nothing in the library is lost).
- `RemoteLibrary`: the download cache is never evicted and isn't deleted when the library is removed; there's a 10 s connect timeout but no overall request timeout.
- Passwords in `/data/data.json` are plain text (by choice): anyone who can read that folder or its backups sees them. Don't reuse a password that matters.
- **Login tokens are stored in plain text on the devices:** in the browser's `localStorage` (readable by any script running in the page) and in the app's private data (`app_data.json`). Anyone with a copy can use the session until it expires or the password is changed. An OS keyring for the app is a possible follow-up.
- **The login lockout is per username:** someone guessing a user's password keeps that user locked out a minute at a time.
- **Sign-up reveals which usernames exist:** asking for a taken name is an instant 409, to anyone.
- **Strangers can fill the sign-up queue:** at most 20 requests wait at once, so anyone who can reach the server can block new sign-ups until an admin denies theirs. Approved users get the password the requester chose, stored in plain text like every other.
- **`data.json` races:** a hand edit saved in the same instant as a server write (a login, or a session's daily extension) can be lost. The server re-reads the file before every write, so the window is tiny. A field the server doesn't know (a typo such as `pasword`, or one from a newer build) is dropped the next time the server writes the file.
- **A scope change shows after a refetch:** it applies from the user's next request, but listings they already have open stay until they navigate or reload. A book they're reading that's now hidden stays open until then; its progress saves become 404s.
- **A scoped admin sees the whole library in *Settings → Admin*:** the folder picker lists every folder and other users' roots are full paths, so the admin's own scope doesn't hide folder names from them (by choice).
- **A role change shows after a reload:** the frontend caches the user's own role (`me`) for the whole visit, so someone promoted doesn't see the new buttons, and someone demoted still sees buttons that now give a 403, until they reload or switch library. The server's checks apply at once. A ban isn't affected: it signs the user out.
- Removing a user from `data.json` signs them out on their next request, but the app's remote library just shows the login screen; their `.shelf/users/<user>/` folder stays.
- The server's library path (`/books`) and bind address (`0.0.0.0:8080`) are hard-coded.
- Reader zoom doesn't scale images: they get the inverse zoom so foliate's px height cap still fits them to the page. An image sized in px in the book's CSS stays that size at any zoom.
- The interface scale doesn't move breakpoints: media queries (`md:`, Settings' `48rem`) use the browser's default font size, not the root's. At 150% a narrow desktop window keeps the wide layout with bigger controls. Bits sized in px (the 3px position lines, borders) don't scale.

## Platform limits

- **Android folder picker** only maps folders on device storage (`com.android.externalstorage.documents`); other document providers are rejected.
- `MANAGE_EXTERNAL_STORAGE` is restricted on the Play Store. Fine for sideloading, which is the plan. Play Protect may still flag the sideloaded APK; *Install anyway* or `adb install` gets past it, and Google's free hobbyist developer account (up to 20 devices) registers the app.
- **Windows (not a target):** a location's `path` comes from `Path::to_string_lossy`, so on Windows it would hold `\` separators, breaking folder listings (which split on `/`) and libraries shared with other devices. Normalize to `/` before any Windows build.

## Cleanups

- `shelf-core` has pre-existing clippy warnings.
- `core` uses blocking `std::fs` (through `spawn_blocking`), against the "tokio everywhere" rule.
- Unused frontend code and deps: `bookTitle` in `src/lib/book.ts`, `src/components/ui/sheet.tsx`, the npm packages `@tauri-apps/plugin-http` and `@tauri-apps/plugin-store` (the app uses both plugins from Rust only).
- Unused proto: the `FolderListing` message in `bookkeeperpb/metadata.proto`; `DeveloperConfig.logging` in `configpb/config.proto` (the app keeps its log-file count in `AppData`).
- `crates/core/src/library_old.rs` isn't declared in `lib.rs`, so it never compiles.
- The demo's `debounce` never returns: it calls `tx.closed()` without awaiting it and never drops `tx`, so the `Debounced` task waits forever.
- `app/Cargo.toml` still has template metadata (`description = "A Tauri App"`, `authors = ["you"]`).
- `frontend/README.md` is the Vite template's README.

## S3 storage

`S3LibraryStorage` (`crates/core/src/storage/s3_storage.rs`) isn't used yet ([roadmap](roadmap.md#library-core)). Once it is:

- **Empty folders** are an empty `<folder>/.keep` object (`create_folder`), since object stores have no folders. Uploads are a single PUT.
- **No watcher.** Object stores send no change events, so books added or removed in the bucket only show up at the next startup sync or *Rebuild index*.
- **Startup costs requests per book:** one HEAD for its mtime and one GET for its `metadata.json`, on top of the LIST calls. A book that has to be hashed is downloaded whole.
- **Covers and books are buffered in memory** (`FileContent::Bytes`): the server sends them whole, with no Range requests. A `FileContent::Stream` variant fed into axum's `Body::from_stream` would fix that for the server; the app reads whole books anyway.
