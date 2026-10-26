# API

The same operations exist as HTTP routes (`shelf-server`) and as Tauri commands (the app). Both carry the same JSON.

## Wire conventions

- **Schema:** `proto/`. Packages: `apipb` (requests/responses, `ApiError`), `bookkeeperpb` (on-disk metadata and progress), `configpb` (library settings), `statepb` (on-disk state). Package names have no version suffix.
- **Encoding:** proto3 JSON, never binary protobuf.
  - Keys are the **snake_case proto field names** (`directory_path`, not `directoryPath`). The frontend encodes with `toJson(..., { useProtoFieldName: true })`; Rust's pbjson is built with `preserve_proto_field_names()`.
  - Enums travel as their names (`"PAGINATED"`).
  - Unknown fields are ignored on decode, on both sides.
  - Avoid 64-bit integer fields: proto3 JSON sends them as strings.
- **Enums live in wrapper messages** (`ReaderFlow.Value`) so their values stay plain names like `LIGHT` rather than `READER_FLOW_LIGHT`. `buf.yaml` disables the lint rules that would object.
- **Raw bytes exception:** covers and EPUB files are sent as raw bytes, not JSON.
- **Changing the schema:** edit `.proto` files only, never renumber or reuse a field number, then rebuild (Rust regenerates on `cargo build`; the frontend on `pnpm dev`/`build`/`test`). Run `buf lint`.
- **Registering a new file:** `crates/proto/build.rs` has an explicit list of `.proto` files. Add new ones there. (`proto/apipb/cover.proto` is empty and isn't in the list.)

## HTTP routes (`crates/server/src/api.rs`)

All under `/api`. JSON routes are `POST` with a JSON body, even reads, so every request has the same shape.

| Method | Path | Request body | Response |
|---|---|---|---|
| GET | `/api/health` | — | `200 ok` (plain text) |
| POST | `/api/books/list` | `ListBooksRequest` `{directory_path}` | `ListBooksResponse` `{current_directory, books: [{id, file_name, title?, authors}]}` |
| POST | `/api/directories/list` | `ListDirectoriesRequest` `{directory_path}` | `ListDirectoriesResponse` `{current_directory, sub_directories: [name]}` |
| POST | `/api/progress/get` | `GetProgressRequest` `{id}` | `GetProgressResponse` `{id, epub_progress?}` |
| POST | `/api/progress/set` | `SetProgressRequest` `{id, epub_progress?}` | `204` |
| POST | `/api/state/get` | `GetStateRequest` `{}` | `GetStateResponse` `{last_book_id?}` |
| POST | `/api/state/set` | `SetStateRequest` `{last_book_id?}` | `204` |
| POST | `/api/config/get` | none | `Config` |
| POST | `/api/config/set` | `Config` (a partial patch) | `Config` (the whole merged config) |
| POST | `/api/library/rebuild` | `RebuildIndexRequest` `{}` | `RebuildIndexResponse` `{books, failed, removed}` |
| GET | `/api/books/{id}/cover` | — | JPEG bytes |
| GET | `/api/books/{id}/file` | — | EPUB bytes |

Notes:
- `directory_path` is relative to the library root, `""` is the root; leading and trailing slashes are trimmed. Books are sorted by title (else file name), folders by name, both case-insensitively.
- `/cover` and `/file` use tower-http `ServeFile` (Range, `Last-Modified`, content type) and add `Cache-Control: no-store`.
- `set_progress` without `epub_progress` is a no-op `204`.
- `/library/rebuild` answers only when the rebuild is done, which takes as long as reading every book file. A second rebuild waits for the first.
- Every non-API path is the SPA (`crates/server/src/frontend.rs`). Unknown `/api/*` paths return a JSON 404.

### Errors

Failures return an `ApiError` body `{ "message": "…" }`:

| Cause | Status | Message |
|---|---|---|
| `LibraryError::NotFound` (unknown book ID, book without a cover, unknown API path) | 404 | `not found` |
| `LibraryError::Internal` | 500 | `internal error` (details only in the server log) |
| Malformed JSON body | axum's default 4xx rejection (plain text, not `ApiError`) | — |

There is **no authentication**. Run the server only on a trusted network (LAN, Tailscale) or behind a proxy that authenticates.

## Tauri commands (`app/src/commands.rs`, `app/src/native.rs`)

Arguments are passed as an object. Tauri maps JS camelCase argument names to Rust snake_case parameter names (`baseUrl` → `base_url`); the proto payloads inside `req`/`patch` keep snake_case keys.

### Library passthroughs (need an open library)

| Command | Args | Resolves to |
|---|---|---|
| `list_books` | `{ req: ListBooksRequest }` | `ListBooksResponse` |
| `list_directories` | `{ req: ListDirectoriesRequest }` | `ListDirectoriesResponse` |
| `get_progress` | `{ req: GetProgressRequest }` | `GetProgressResponse` |
| `set_progress` | `{ req: SetProgressRequest }` | `null` |
| `get_state` | `{ req: GetStateRequest }` | `GetStateResponse` |
| `set_state` | `{ req: SetStateRequest }` | `null` |
| `get_config` | none | `Config` |
| `set_config` | `{ patch: Config }` | `Config` |
| `rebuild_index` | none | `RebuildIndexResponse`. For a remote library, the server rebuilds its own index |
| `get_cover` | `{ id }` | `ArrayBuffer` (JPEG) |
| `get_book_file` | `{ id }` | `ArrayBuffer` (EPUB) |

`get_cover` / `get_book_file` read the whole file and return `tauri::ipc::Response::new(bytes)`, which reaches JS as an `ArrayBuffer`. A custom URI scheme (`cover://`) was tried first and dropped: WebKitGTK doesn't dispatch custom-scheme subresource loads from a page served over `http://` (the Vite dev server), so covers never loaded in `cargo tauri dev` on Linux.

### Library management

| Command | Args | Resolves to | Notes |
|---|---|---|---|
| `list_libraries` | none | `{ current, libraries: LibraryInfo[] }` | `LibraryInfo` is `{ name, kind: "local" \| "remote", location }`; `current` is a name or `null` |
| `add_local_library` | `{ name, path }` | `LibraryInfo` | Canonicalizes the path, requires a folder; becomes current |
| `add_remote_library` | `{ name, baseUrl }` | `LibraryInfo` | Prepends `http://` if there's no `://`; only http/https; becomes current |
| `select_library` | `{ name }` | `null` | Makes it current (persisted); doesn't open it |
| `remove_library` | `{ name }` | `null` | Forgets it on this device; closes it if open; nothing in the library is deleted |
| `open_library` | `{ name }` | `null` | Resolves after a local library's startup sync |

Names are required and unique ignoring case. An empty or duplicate name, an unreadable folder or a malformed URL rejects with status 400.

### Native helpers (`app/src/native.rs`)

| Command | Android | Elsewhere |
|---|---|---|
| `storage_granted` | whether all-files access (API 30+) or `READ_EXTERNAL_STORAGE` is granted | `true` |
| `request_storage` | opens the settings page / permission prompt, resolves with the outcome on return | `true` |
| `pick_folder` | system folder picker, mapped to a filesystem path; rejects when cancelled or not on device storage | rejects |
| `system_insets` | `{top, right, bottom, left}` of status bar, nav bar and cutout, in CSS px | all `0` |

### Errors

A failed command rejects with `CommandError` `{ status, message }`, the same shape as the server's errors plus a status. `NotFound` → `404 "not found"` (logged at debug). Bad input to the library-management commands (name, folder, URL) → `400` with the reason (logged at debug, `CommandError::invalid`). Anything else → `500` with the full `anyhow` chain as the message (logged at error). Unlike the server, the app shows the real message, since the user owns the device. "no library is open" is a 500 from any passthrough called before `open_library`.

The frontend's `invokeCommand` (`frontend/src/api/tauri.ts`) turns rejections into `ApiError(status, message)`, the same class the HTTP backend throws.

### Events

| Event | Payload | When |
|---|---|---|
| `library-changed` | `null` | A local library's watcher synced a book (added, changed, moved or removed). Remote libraries never emit it. |
