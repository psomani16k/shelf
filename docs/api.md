# API

The same operations exist as HTTP routes (`shelf-server`) and as Tauri commands (the app). Both carry the same JSON.

## Wire conventions

- **Schema:** `proto/`. Packages: `apipb` (requests/responses, `ApiError`, and `Role`, which `serverpb` imports), `bookkeeperpb` (on-disk metadata and progress), `configpb` (each user's settings), `statepb` (on-disk state), `serverpb` (the server's `data.json`; server-only, not generated for the frontend). Package names have no version suffix.
- **Encoding:** proto3 JSON, never binary protobuf.
  - Keys are the **snake_case proto field names** (`directory_path`, not `directoryPath`). The frontend encodes with `toJson(..., { useProtoFieldName: true })`; Rust's pbjson is built with `preserve_proto_field_names()`.
  - Enums travel as their names (`"PAGINATED"`).
  - Unknown fields are ignored on decode, on both sides.
  - Avoid 64-bit integer fields: proto3 JSON sends them as strings.
- **Enums live in wrapper messages** (`ReaderFlow.Value`) so their values stay plain names like `PAGINATED` rather than `READER_FLOW_PAGINATED`. `buf.yaml` disables the lint rules that would object.
- **Raw bytes exception:** covers and EPUB files are sent as raw bytes, not JSON: downloads as the response, uploads as the request body (with the request in the query).
- **Changing the schema:** edit `.proto` files only, never renumber or reuse a field number, then rebuild (Rust regenerates on `cargo build`; the frontend on `pnpm dev`/`build`/`test`). Run `buf lint`.
- **Registering a new file:** `crates/proto/build.rs` has an explicit list of `.proto` files. Add new ones there. (`proto/apipb/cover.proto` is empty and isn't in the list.)

## HTTP routes (`crates/server/src/api.rs`)

All under `/api`. JSON routes are `POST` with a JSON body, even reads, so every request has the same shape (`/auth/me` is the one `GET`).

**Every route except `health`, `auth/login` and `auth/signup` needs `Authorization: Bearer <token>`**, the token from `auth/login`; covers and files too. Without a valid one it's a 401 before the handler runs (and before an upload's body is read). The browser keeps the token in `localStorage`, the app in its private data; there are no cookies.

| Method | Path | Request body | Response |
|---|---|---|---|
| GET | `/api/health` | — | `200 ok` (plain text) |
| POST | `/api/auth/login` | `LoginRequest` `{username, password}` | `LoginResponse` `{token, user: {username, role}}` |
| POST | `/api/auth/logout` | — | `204`; ends this token's session |
| GET | `/api/auth/me` | — | `User` `{username, role}` |
| POST | `/api/auth/password` | `ChangePasswordRequest` `{current_password, new_password}` | `204` |
| POST | `/api/auth/signup` | `SignUpRequest` `{username, password}` | `204`; the request waits for an admin |
| POST | `/api/users/requests/list` | `ListUserRequestsRequest` `{}` | `ListUserRequestsResponse` `{requests: [{id, username, created, taken}]}`, oldest first |
| POST | `/api/users/requests/approve` | `ApproveUserRequestRequest` `{id, role}` | `204`; the request becomes a user |
| POST | `/api/users/requests/deny` | `DenyUserRequestRequest` `{id}` | `204`; the request is deleted |
| POST | `/api/users/list` | `ListUsersRequest` `{}` | `ListUsersResponse` `{users: [{username, role}], scopes: {<username>: {library_root, books}}}`, by name |
| POST | `/api/users/role/set` | `SetUserRoleRequest` `{username, role}` | `204` |
| POST | `/api/users/scope/set` | `SetUserScopeRequest` `{username, scope: {library_root, books}}` | `204` |
| POST | `/api/users/scope/directories` | `ListDirectoriesRequest` `{directory_path}` | `ListDirectoriesResponse`, of the whole library |
| POST | `/api/books/list` | `ListBooksRequest` `{directory_path}` | `ListBooksResponse` `{current_directory, books: [{id, file_name, title?, authors}]}` |
| POST | `/api/directories/list` | `ListDirectoriesRequest` `{directory_path}` | `ListDirectoriesResponse` `{current_directory, sub_directories: [name]}` |
| POST | `/api/progress/get` | `GetProgressRequest` `{id}` | `GetProgressResponse` `{id, epub_progress?}` |
| POST | `/api/progress/set` | `SetProgressRequest` `{id, epub_progress?}` | `204` |
| POST | `/api/state/get` | `GetStateRequest` `{}` | `GetStateResponse` `{last_book_id?}` |
| POST | `/api/state/set` | `SetStateRequest` `{last_book_id?}` | `204` |
| POST | `/api/config/get` | `GetConfigRequest` `{}` | `Config` |
| POST | `/api/config/set` | `SetConfigRequest` `{patch}` (`patch` is a partial `Config`) | `Config` (the user's whole merged config) |
| POST | `/api/library/rebuild` | `RebuildIndexRequest` `{}` | `RebuildIndexResponse` `{books, failed, removed}` |
| POST | `/api/library/progress/clean` | `CleanUpProgressRequest` `{}` | `CleanUpProgressResponse` `{books}` (folders deleted) |
| POST | `/api/books/upload?parent_directory=…&file_name=…` | the EPUB's raw bytes (`UploadEbookRequest` is the query) | `UploadEbookResponse` `{id, file_name}` (the name it got) |
| POST | `/api/directories/create` | `CreateDirectoryRequest` `{parent_directory, directory_name}` | `CreateDirectoryResponse` `{directory_name}` (the name it got) |
| POST | `/api/library/delete` | `DeleteItemsRequest` `{directory_paths, books: [{id, directory_path, file_name}]}` | `DeleteItemsResponse` `{books, directories}` (how many were deleted) |
| GET | `/api/books/{id}/cover` | — | JPEG bytes |
| GET | `/api/books/{id}/file` | — | EPUB bytes |

Notes:
- **Auth** (`crates/server/src/auth.rs`; sessions are stored in `/data/data.json`, see [server.md](server.md#server-data)):
  - **Login:** the password is compared in constant time; an unknown username is compared against a dummy, so both take as long. Wrong username or password: 401 `wrong username or password`. A user with no password set can't log in. A banned user (`BANNED`, also the value when `role` is left out) with the right password gets a 403 `this account is banned; contact an admin of this server`, checked after the password so it never confirms a name to a guesser. A name that isn't a user but has a pending sign-up with that password gets a 403 `this account is waiting for an admin's approval`. The token is 32 random bytes from the OS, base64url; only its hex SHA-256 is stored, with `created` and `expires` (90 days).
  - **Lockout:** 5 wrong passwords in a row (login, or the current password in `auth/password`) lock that username for 60 s: 429 `too many wrong passwords; try again in a minute`. A success resets the count. Counted per existing user, in memory; unknown names aren't tracked.
  - **Sessions slide:** a request with a session that has less than 89 days left pushes it back to 90, so it's written at most once a day per session. An expired or unknown token, or one whose user has since been banned, is a 401 `this session has ended; sign in again`; a missing one `sign in first`.
  - **The role is read from `data.json` on every request** (it's reloaded when it changes), so a role change or a removed user applies to the next request.
  - **Change password:** a wrong `current_password` is a 400 (not 401, which would sign the client out); `new_password` must be 1 to 256 characters and isn't trimmed. Every other session of the user ends; this one stays. Usernames can't be changed.
  - **Logout** ends only this token's session.
- **Sign-up** (`crates/server/src/auth/requests.rs`): anyone can ask for an account; an admin or owner approves it, choosing the role, or denies it. Requests are kept in `data.json` → `user_requests` ([server.md](server.md#server-data)).
  - **Asking:** the username must be valid (`validate_user`; else 400 `a username is 1 to 32 of a–z, 0–9, - and _`), the password 1 to 256 characters (else 400). A name a user already has is a 409 `this username is already taken`. Names are unique among users only: two pending requests may share one. At most 20 requests wait at once (`MAX_PENDING_REQUESTS`); then it's a 429 `too many accounts are waiting for approval; try again later`. Each request gets a random id (16 bytes, base64url) and its `created` time.
  - **Listing** never sends the password. `taken` is true when a user has the name by now (another request with it was approved, or it was added by hand); the request can then only be denied.
  - **Approving:** the role must be `READER`, `CONTRIBUTOR` or `LIBRARIAN`, or `ADMIN` when the approver is an `OWNER`; anything else is a 403 `you can't give the X role`. The name is checked again against `users` in the file as it is at that moment, so two users never share a name: a taken one is a 409 `this username is already taken; deny the request`, and the request stays. An unknown id is a 404. On success the request is removed and the user added with its password and the role. Nothing is written when a check fails (`DataFile::try_update`).
  - **Denying** deletes the request; an unknown id is a 404.
- **Roles** (`crates/server/src/auth/users.rs`): `users/list` sends every user's name and role, never passwords or sessions. `users/role/set` checks, in order: changing your own role is a 403 `you can't change your own role`; a role the caller may not give is a 403 `you can't give the X role`; an unknown user is a 404; a user whose current role the caller may not take away is a 403 `you can't change an X's role`. The rule (`may_manage`, shared with approving): `BANNED`, `READER`, `CONTRIBUTOR` and `LIBRARIAN` need `ADMIN` or higher, `ADMIN` needs `OWNER`, and `OWNER` is never given or taken. Banning also deletes all of the user's sessions, so they're signed out at once; unbanning doesn't bring them back. Nothing is written when a check fails.
- **Scopes** (`users.rs`): `users/list` also sends each user's `library_root` and `books` in `scopes` (not on `User`, which `auth/me` sends the user themselves). `users/scope/set` normalizes the scope (the root goes through the folder-path rules, `/kid/` → `kid`; patterns are trimmed and blank ones dropped) and checks it like `data.json` does: a bad root or pattern is a 400 with the parser's message, and a root that isn't a folder in the library a 400 `there's no folder "x"`. Then, as for roles: your own is a 403, an unknown user a 404, and the caller must be allowed to manage the user's current role (`may_manage`), so admins set the scopes of users below admin, owners admins' too, and an owner's is only set by hand. It applies from the user's next request; their sessions stay. `users/scope/directories` lists folders of the whole library for picking a root. **An admin's own scope doesn't apply to either**: a scoped admin sees and sets full library paths (a choice; see [decisions.md](decisions.md)).
- **Scopes** (`crates/core/src/library/scope.rs`, `view.rs`): each user sees only their part of the library, set in `/data/data.json` by `library_root` and `books` ([server.md](server.md#server-data)). Every handler that touches books, folders, progress or the last book goes through `ServerLibrary::view(&user.scope)`, a `LibraryView`. A user without either field sees the whole library.
  - **Paths are relative to the user's root**, both ways: their root is `""`, and nothing they get back names it, so the UI shows it as the library root.
  - **A hidden book acts as missing:** a book is visible when one of its copies is under the root and allowed by the patterns. A hidden one is left out of listings, and its cover, file and progress are 404s; a last book that's hidden reads as unset. A copy outside the scope is never deleted.
  - **A hidden folder acts as missing:** with patterns, a folder whose books (as the index sees them) are all hidden is left out of folder listings, lists as empty, and is a 400 as a parent. Folders with no books always show.
  - **No leaks:** an upload never answers differently because a hidden file is there (see Upload).
- `directory_path` is relative to the user's root, `""` is the root; leading and trailing slashes are trimmed. Books are sorted by title (else file name, which also orders copies of one book), folders by name, both case-insensitively. Each copy of a book is its own item, so two copies in one folder are two items with the same `id`. Books come from the in-memory index; folders are read from disk (`LibraryStorage::sub_folders`), so empty folders and folders without books are listed too. Dot-folders never are.
- **Names and folder paths from clients** (`crates/core/src/library/names.rs`): a file or folder name is trimmed, can't be empty, longer than 255 bytes, contain `/`, `\` or NUL, or start with a dot (so nothing hidden, and never `.shelf/`). A folder path is names joined by `/`. Breaking a rule is a 400.
- **Upload** (`LibraryView::upload_book`): the name must end in `.epub`, and the folder must exist and be visible (else 400). The names tried are `<name>`, then `<stem> (1).epub` up to `(99)`. One the scope doesn't allow is skipped whether or not a file has it, so the answer never shows that a hidden file is there. The first allowed name that's free is used; one taken by a book or folder the user can see is a 409 (`<path> already exists`); one taken by something they can't see (a file that isn't indexed) moves on to the next. No allowed name is a 403 `books can't be added here`. A book already in the library is fine: the upload becomes another copy (location) of it. The file is written as a hidden `.<name>.part-<pid>-<seq>` next to the target and renamed over it, then synced into the index at once (`IndexSync::sync_path`), so it's in the next listing; the watcher's later event finds it unchanged. It must then be in the index at this path (another copy being indexed isn't enough); a file that fails extraction is deleted again and gets a 400 (`<name> isn't a readable EPUB`). The body limit is 512 MiB (`MAX_UPLOAD_BYTES`; axum's default of 2 MB applies to every other route); a bigger one gets axum's plain-text 413. The whole book is held in memory while it's stored.
- **Create folder:** the parent must exist and be visible (400). A name taken by a folder or book the user can see is a 409; one taken by anything else (a hidden folder, a non-book file) moves on to `<name> (1)` and so on. The response has the name it got.
- **Delete** (`LibraryView::delete_items`): every path is checked first, so a bad request deletes nothing; the user's root is a 400. Each `books` entry deletes exactly the copy named by `directory_path` and `file_name`, if it's that book's; unknown ids and copies, and copies the user can't see, are skipped, and a hidden folder deletes nothing. A folder goes with everything in it, non-book files included, unless it holds a book the user can't see (checked on disk, so a book not indexed yet counts): then only its visible books are deleted, the folder stays, and it's still counted. Deleting a copy keeps the book (and every user's progress) while another copy is left. What was deleted leaves the index at once; a book left with no copies keeps its `metadata.json` and thumbnail until the next startup sync deletes them, as for a book deleted outside Shelf; its progress is kept, so adding the book back resumes it. If one delete fails, the ones before it stay done and the request is a 500.
- Upload, create and delete hold `BookLibrary`'s `files_lock`, so their checks and changes don't interleave.
- `/cover` and `/file` add `Cache-Control: no-store`. A local file (`FileContent::Local`) goes through tower-http `ServeFile` (Range, `Last-Modified`, content type); bytes (`FileContent::Bytes`, from storage without local files, such as S3, which isn't wired up) are sent whole as `image/jpeg` / `application/epub+zip`, with no Range support. A book still in the index whose file is gone from disk gets `ServeFile`'s 404 with an empty body.
- `set_progress` without `epub_progress` is a no-op `204`.
- Progress, the last book and settings are per user. Their requests (`GetProgressRequest`, `SetProgressRequest`, `GetStateRequest`, `SetStateRequest`, `GetConfigRequest`, `SetConfigRequest`) also have a `user` field, which clients leave out: it's filled in where a request enters, never taken from the client: the server's handlers set it to the session's user, and the app's `LocalLibrary` sets it to the library entry's `user` from the app data. A remote library sends whatever the frontend sent, and the server replaces it. `BookLibrary` rejects an invalid name with a 500.
- `get_state` returns a `last_book_id` that's no longer in the library as unset.
- `/library/rebuild` answers only when the rebuild is done, which takes as long as reading every book file. A second rebuild waits for the first.
- `/library/progress/clean` deletes the `.shelf/books/<id>/` folders of books no file has any more (no `metadata.json`, not in the index), every user's progress included ([library-format.md](library-format.md#cleaning-up-progress)). It waits for a running rebuild, upload or watcher job.
- Every non-API path is the SPA (`crates/server/src/frontend.rs`). Unknown `/api/*` paths return a JSON 404. A known path with the wrong method gets axum's `405` with an empty body.

### Access by role

Roles are defined in [server.md](server.md#server-data); each can do everything the ones before it can (`READER` < `CONTRIBUTOR` < `LIBRARIAN` < `ADMIN` < `OWNER`), so access is a minimum role. `BANNED` can't sign in, so it can call nothing but `health`, `auth/login` (which tells it so) and `auth/signup`. A logged-in user below it gets a 403 (`this needs the LIBRARIAN role or higher`). `api::router` groups the routes by that minimum, with `auth::require` as a `route_layer` on the contributor, librarian and admin groups.

| Route | Tauri command | Status | READER | CONTRIBUTOR | LIBRARIAN | ADMIN | OWNER |
|---|---|---|:-:|:-:|:-:|:-:|:-:|
| `GET /api/health` | — | built | ✓ | ✓ | ✓ | ✓ | ✓ |
| `POST /api/books/list` | `list_books` | built | ✓ | ✓ | ✓ | ✓ | ✓ |
| `POST /api/directories/list` | `list_directories` | built | ✓ | ✓ | ✓ | ✓ | ✓ |
| `GET /api/books/{id}/cover` | `get_cover` | built | ✓ | ✓ | ✓ | ✓ | ✓ |
| `GET /api/books/{id}/file` | `get_book_file` | built | ✓ | ✓ | ✓ | ✓ | ✓ |
| `POST /api/progress/get`, `/set` | `get_progress`, `set_progress` | built | ✓ own | ✓ own | ✓ own | ✓ own | ✓ own |
| `POST /api/state/get`, `/set` | `get_state`, `set_state` | built | ✓ own | ✓ own | ✓ own | ✓ own | ✓ own |
| `POST /api/config/get`, `/set` | `get_config`, `set_config` | built | ✓ own | ✓ own | ✓ own | ✓ own | ✓ own |
| `POST /api/library/rebuild` | `rebuild_index` | built | | | | ✓ | ✓ |
| `POST /api/library/progress/clean` | `clean_up_progress` | built | | | | ✓ | ✓ |
| `POST /api/auth/login` | `login` | built | ✓ | ✓ | ✓ | ✓ | ✓ |
| `POST /api/auth/logout`, `GET /api/auth/me`, `POST /api/auth/password` | `logout`, `me`, `change_password` | built | ✓ own | ✓ own | ✓ own | ✓ own | ✓ own |
| `POST /api/books/upload` | `upload_book` | built | | ✓ | ✓ | ✓ | ✓ |
| `POST /api/directories/create` | `create_directory` | built | | ✓ | ✓ | ✓ | ✓ |
| `POST /api/library/delete` | `delete_items` | built | | | ✓ | ✓ | ✓ |
| `POST /api/auth/signup` | `sign_up` | built | anyone, signed in or not | | | | |
| `POST /api/users/requests/list`, `/deny` | `list_user_requests`, `deny_user_request` | built | | | | ✓ | ✓ |
| `POST /api/users/requests/approve` as `READER`, `CONTRIBUTOR` or `LIBRARIAN` | `approve_user_request` | built | | | | ✓ | ✓ |
| `POST /api/users/requests/approve` as `ADMIN` | `approve_user_request` | built | | | | | ✓ |
| `POST /api/users/list` | `list_users` | built | | | | ✓ | ✓ |
| `POST /api/users/role/set` between `READER`, `CONTRIBUTOR`, `LIBRARIAN` and `BANNED`, for a user who has one of those | `set_user_role` | built | | | | ✓ other users | ✓ other users |
| `POST /api/users/role/set` to or from `ADMIN` | `set_user_role` | built | | | | | ✓ other users |
| `POST /api/users/scope/set` for a `READER`, `CONTRIBUTOR`, `LIBRARIAN` or `BANNED` user | `set_user_scope` | built | | | | ✓ other users | ✓ other users |
| `POST /api/users/scope/set` for an `ADMIN` | `set_user_scope` | built | | | | | ✓ other users |
| `POST /api/users/scope/directories` | `list_scope_directories` | built | | | | ✓ | ✓ |
| Remove a user below admin | — | planned | | | | ✓ | ✓ |
| Remove an admin | — | planned | | | | | ✓ |
| Make or remove an owner | — | — | | | | | |

- `health`, `login` and `signup` need no session; every other route does, and gets a 401 without one.
- "Own" progress, last book and settings: the user comes from the session, so a user can only read and write their own (see the note above). Nobody, not even an owner, reads another user's through the API.
- Owners are made and removed only by editing `/data/data.json` on the server, never through any route.
- An admin can't touch admins or owners, including making a reader into an admin.
- Scopes apply to every role. *Rebuild index* and *Clean up progress* cover the whole library whoever asks, so a scoped admin's counts include books they can't see.
- **Tauri commands:** for a remote library these are forwarded to the server, which enforces the same table. A local library has no roles: every command is allowed ([decisions.md](decisions.md)). Device-only commands (the library list, logs, interface scale, native helpers) aren't library operations and are never role-checked.

### Errors

Failures return an `ApiError` body `{ "message": "…" }`:

| Cause | Status | Message |
|---|---|---|
| `LibraryError::NotFound` (unknown book ID, including `set_state`'s `last_book_id`; book without a cover; unknown API path) | 404 | `not found` |
| `LibraryError::Invalid` (a bad name or folder path, a missing folder, an upload that isn't a readable EPUB) | 400 | what was wrong |
| `LibraryError::Conflict` (the name is taken by something the caller can see) | 409 | what's in the way |
| `LibraryError::Unauthorized` (no token, or its session has ended; wrong login) | 401 | what to do |
| `LibraryError::Forbidden` (the role is too low, an upload to where the caller's scope allows no books, a role the caller can't give or take, or a login that's banned or still waiting for approval) | 403 | the role needed, or why |
| `LibraryError::TooManyRequests` (locked after wrong passwords) | 429 | when to try again |
| `LibraryError::Internal` | 500 | `internal error` (details only in the server log) |
| Malformed JSON body | axum's default 4xx rejection (plain text, not `ApiError`) | — |

Tokens and passwords travel in the clear unless the connection is HTTPS: run the server behind a TLS proxy (or on Tailscale) when it's reachable beyond your own machine.

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
| `get_config` | `{ req: GetConfigRequest }` | `Config` |
| `set_config` | `{ req: SetConfigRequest }` | `Config` |
| `rebuild_index` | none | `RebuildIndexResponse`. For a remote library, the server rebuilds its own index |
| `clean_up_progress` | none | `CleanUpProgressResponse`. For a remote library, the server cleans up its own |
| `get_cover` | `{ id }` | `ArrayBuffer` (JPEG) |
| `get_book_file` | `{ id }` | `ArrayBuffer` (EPUB) |
| `create_directory` | `{ req: CreateDirectoryRequest }` | `CreateDirectoryResponse` |
| `delete_items` | `{ req: DeleteItemsRequest }` | `DeleteItemsResponse` |
| `upload_book` | `{ req: UploadEbookRequest, uploadId, offset, data, done }` | `null`, then `UploadEbookResponse` for the call with `done` |
| `login` | `{ req: LoginRequest }` | `User`. Stores the token in the library's entry in the app data and uses it at once |
| `logout` | none | `null`. Tells the server when it can, and always forgets the token |
| `me` | none | `User`, or `null` for a local library |
| `change_password` | `{ req: ChangePasswordRequest }` | `null` |

A remote library sends its stored token as `Authorization: Bearer` on every request, covers and files included. `login`, `logout` and `change_password` on a local library are a 400 (`a local library has no accounts`); a local library has no roles, so every other command is allowed.

`get_cover` / `get_book_file` read the whole file (or take the bytes, for storage without local files) and return `tauri::ipc::Response::new(bytes)`, which reaches JS as an `ArrayBuffer`. A custom URI scheme (`cover://`) was tried first and dropped: WebKitGTK doesn't dispatch custom-scheme subresource loads from a page served over `http://` (the Vite dev server), so covers never loaded in `cargo tauri dev` on Linux.

`upload_book` takes the book in chunks: `data` is up to 2 MiB of it in base64, at byte `offset`. Raw bytes would be cheaper, but IPC on Linux and Android falls back to `postMessage`, which sends them as a JSON array of numbers, and one call per book would give no progress. Each chunk is appended to `<app cache dir>/uploads/<uploadId>.part`: `offset` 0 starts the file, any other offset must be its length so far (else a 400), and `uploadId` (the frontend's `crypto.randomUUID()`) may only hold hex digits and `-`. The call with `done` reads the file, deletes it, and calls the library's `upload_book`, which for a remote library posts it to the server's upload route. `setup` deletes the `uploads/` folder on start, so a cut-off upload doesn't stay. The errors are the server's: 400 and 409 keep their messages through a remote library too.

### Library management

| Command | Args | Resolves to | Notes |
|---|---|---|---|
| `list_libraries` | none | `{ current, libraries: LibraryInfo[] }` | `LibraryInfo` is `{ name, kind: "local" \| "remote", location, signed_in }` (`signed_in`: a remote library has a token; always `true` for a local one); `current` is a name or `null` |
| `add_local_library` | `{ name, path }` | `LibraryInfo` | Canonicalizes the path, requires a folder; becomes current |
| `add_remote_library` | `{ name, baseUrl }` | `LibraryInfo` | Prepends `http://` if there's no `://`; only http/https; becomes current |
| `select_library` | `{ name }` | `null` | Makes it current (persisted); doesn't open it |
| `remove_library` | `{ name }` | `null` | Forgets it on this device; closes it if open; nothing in the library is deleted. A signed-in remote library is also logged out on the server, in the background |
| `open_library` | `{ name }` | `null` | Resolves after a local library's startup sync |

Names are trimmed, required and unique ignoring case. An empty or duplicate name, an unreadable folder or a malformed URL rejects with status 400. An unknown name passed to `select_library`, `remove_library` or `open_library` rejects with status 500.

### Interface scale

App only (`app/src/commands.rs`). Per device, in the app data; see [app.md](app.md#stored-data).

| Command | Args | Resolves to | Notes |
|---|---|---|---|
| `ui_scale` | none | a number, in percent | 100 unless changed |
| `set_ui_scale` | `{ percent }` | the saved number | 50 to 200 in steps of 10, else a 400 |

### Logs

App only (`app/src/logging/mod.rs`). See [app.md](app.md#logging).

| Command | Args | Resolves to | Notes |
|---|---|---|---|
| `log_settings` | none | `{ keep_files, max_keep_files }` | `keep_files` counts every log file, the current one included |
| `set_log_files` | `{ keepFiles }` | the same | 1 to `max_keep_files` (50), else a 400; saved in the app data; files past the new count are deleted at once |
| `list_log_files` | none | `{ dir, files: [{ name, bytes, started, last_write, current }] }` | Newest first. `started` is the app start that began the file, `last_write` its mtime (RFC 3339; `last_write` may be `null`); `current` marks this run's file |
| `copy_logs` | none | `null` | The last 256 KiB, starting at a whole line, onto the clipboard |
| `save_logs` | none | `true`, or `false` if cancelled | Save picker, then every log file oldest first into one |
| `save_log_file` | `{ name }` | `true`, or `false` if cancelled | Save picker, then that one file. A name that isn't `shelf-<time>.log` is a 400 |

The frontend's own log lines go through `tauri-plugin-log`'s `plugin:log|log` command (`@tauri-apps/plugin-log`, allowed by `log:default`).

### Native helpers (`app/src/native.rs`)

| Command | Android | Elsewhere |
|---|---|---|
| `storage_granted` | whether all-files access (API 30+) or `READ_EXTERNAL_STORAGE` is granted | `true` |
| `request_storage` | opens the settings page / permission prompt, resolves with the outcome on return | `true` |
| `pick_folder` | system folder picker, mapped to a filesystem path; rejects when cancelled or not on device storage | rejects |
| `system_insets` | `{top, right, bottom, left}` of status bar, nav bar and cutout, in CSS px | all `0` |

### Errors

A failed command rejects with `CommandError` `{ status, message }`, the same shape as the server's errors plus a status. `NotFound` → `404 "not found"` (logged at debug). A remote library's 400, 401, 403, 409 and 429 keep their status and the server's message (debug). Bad input to the library-management commands (name, folder, URL) → `400` with the reason (logged at debug, `CommandError::invalid`). Anything else → `500` with the full `anyhow` chain as the message (logged at error). Unlike the server, the app shows the real message, since the user owns the device. "no library is open" is a 500 from any passthrough called before `open_library`.

The frontend's `invokeCommand` (`frontend/src/api/tauri.ts`) turns rejections into `ApiError(status, message)`, the same class the HTTP backend throws.

### Events

| Event | Payload | When |
|---|---|---|
| `library-changed` | `null` | A local library's watcher sync changed the index (a book or folder added, changed, moved or removed), once per synced path. Remote libraries never emit it. |
