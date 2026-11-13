# Goals and decisions

What Shelf is for, the choices it's built on, and where the original plan (the old `PLAN.md`, removed in commit `aed1ede`) was changed. Unbuilt plans are in [roadmap.md](roadmap.md).

## What it's for

A personal EPUB reader whose library is a plain folder of books, like an Obsidian vault. Built for one person, on two Linux laptops, an Android phone and a browser, with room for a few more people (a household) sharing the same library, each with their own reading progress. Progress is stored per user. The server has logins; choosing a user for an app's local library is planned ([roadmap.md](roadmap.md#users-and-authentication)).

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
- **Live syncing between devices in use at the same time.** Each user reads on one device at a time: progress is read when a book opens. No "progress changed" events and no "jump to newer position" prompts.
- Native Windows, macOS and iOS apps (but don't break them needlessly). Distributing them without warnings needs paid signing: Apple's $99/year developer program (iOS also needs a Mac to build, and free installs expire after 7 days), and Windows code signing (Microsoft's Artifact Signing is open to individuals only in the US and Canada). Not worth it for a personal project. On those devices, use the web UI from `shelf-server` in a browser.
- Play Store distribution. This is what allows the Android all-files-access approach.
- Sharing between users. (Sign-up with an admin's approval, roles, per-user library roots and per-user book access are in scope; see the [roadmap](roadmap.md#users-and-authentication).)

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
| Progress storage | One file per book and user, `books/<id>/progress/<user>.json`, last writer wins | Conflicts are out of scope. Under the book, so it follows the file through moves and copies. When a book is gone, the startup sync deletes only its metadata and thumbnail: progress is small, and keeping it means a book that was removed, briefly missing or re-added resumes where each user left it (at the cost of never cleaning it up). One file per user, so two people reading the same book never conflict in the sync tool |
| Book ID | SHA-1 of the file's bytes | Stable across moves and renames; needs nothing from the EPUB |
| Extracted metadata | Stored in the library's `.shelf/`; deterministic apart from each copy's `last_update_time` | Each book is extracted once, not on every device, and syncs with the library |
| Settings | Per user, `.shelf/users/<user>/config.json`, merged by partial patches | Every setting is a personal preference (reader layout, theme, margins), so each user, readers included, sets their own; stored in the library, so they follow the user to every device |
| App-to-server HTTP | From Rust (`reqwest` via tauri-plugin-http), behind the same `Library` trait as local folders | No CORS or cookie issues in the WebView; the frontend has one backend per platform |
| Covers and books in the app | Raw bytes over IPC (`ipc::Response`) → Blob → object URL | WebKitGTK won't load custom URI schemes from the `http://` dev page |
| Android storage | All files access (`MANAGE_EXTERNAL_STORAGE`) and a small Kotlin plugin, with plain `std::fs` paths | The simplest way to read a synced folder; fine for a sideloaded app |
| App logging | `tracing-subscriber` JSON, as on the server, into Shelf's own log files, one per app start; `tauri-plugin-log` only carries frontend records in | The planned `tauri-plugin-log` formatter would get `tracing` fields flattened into the message text, so they couldn't be top-level keys. Owning the writer also lets a new kept-file count apply without a restart |
| Server users | `/data/data.json`: user id → password, **in plain text**, plus every other per-user server datum (role, sessions, and later library root and book access). The server rewrites it (sessions, password changes), re-reading it first, and reloads it when its mtime changes | The owner's choice for a personal server: easy to edit by hand and nothing to hash. The cost is that anyone who can read the `/data` volume or its backups sees every password, so the file is created with mode `600`, lives outside the library so it never syncs, and is never logged. See [server.md](server.md#server-data) |
| Sign-up | Anyone can ask for an account on the sign-in page; it waits in `data.json` (`user_requests`) until an admin approves it, choosing the role (`READER`, `CONTRIBUTOR` or `LIBRARIAN`; `ADMIN` only from an owner), or denies it | Adding household members shouldn't need a shell on the server, but a public server shouldn't hand out accounts either. Names are unique among users only, checked when asking and again when approving. The instant "already taken" reveals which names exist, accepted for a household server; at most 20 requests wait at once so strangers can't grow the file |
| User roles | One `role` per user, a ladder: `READER` < `CONTRIBUTOR` < `LIBRARIAN` < `ADMIN` < `OWNER`, with `BANNED` (0, the value when `role` is left out) giving no access | A household server needs few distinctions; one ordered value makes each check a single comparison and leaves no contradictory combinations (an earlier draft had a list of overlapping privileges). Adding is split from deleting (`CONTRIBUTOR` adds books and folders, `LIBRARIAN` also deletes) so a household member can grow the library without being able to lose any of it; adding never overwrites, since taken names get ` (n)`. The names avoid publishing terms like writer and editor. Admins' main job will be deciding which books each user can read (see *Per-user scope*). Settings aren't role-gated: they're each user's own. `OWNER` is granted only by editing `data.json`, so the UI can never create or remove one, and none is required. A forgotten `role` locks the user out rather than quietly making them a reader. Admins move other users between `READER`, `CONTRIBUTOR`, `LIBRARIAN` and `BANNED`; only owners make, change or ban admins, and no one changes their own role. A banned user's login is refused with a message saying so (rather than signing in to a "banned" screen), so a banned user never holds a session and no route needs its own ban check; banning deletes their sessions |
| Per-user scope | Each user has a `library_root` (a folder that is their whole library) and `books` (globs for the books they see under it, `!` to exclude, last match wins), set in *Settings → Admin* or in `data.json`. Globs, not regex: they're written as paths, so `scary/**` reads as what it means. Hidden books and folders act as if they don't exist. Enforced in `core` by `LibraryView`, which the server builds per request from the session's user | One server, one index and one `.shelf/` for a household, while each person (a child, say) sees only their part. The user's paths are relative to their root, so being scoped isn't visible. Hidden means missing, not forbidden: a 404 or an empty listing rather than a 403, so nothing confirms a hidden book exists. For the same reason an upload whose name is taken by something the user can't see gets `name (1).epub` instead of a 409, and names the scope hides are skipped whether or not they're taken. Kept out of the `Library` trait, so local libraries and the app don't change. Who may set a scope follows `may_manage`, like roles. The admin UI ignores the admin's own scope (full library paths, a picker over the whole library): simpler, and admins are trusted, at the cost that a scoped admin learns the folder names outside their part and can give others more than they see |
| Identical files | One book (the SHA-1) with a location for each copy, each with its own hash stamp | Users with their own roots will often own the same book. One entry per id keeps progress per book per user and extraction once per book; per-copy stamps keep the startup skip check correct for each copy. Deleting a copy keeps the book while another is left, so nobody else's progress goes with it |
| Server sessions | Opaque 32-byte tokens, sent as `Authorization: Bearer` on every API request (covers and files too). Only their SHA-256 is stored, under the user in `data.json`, with a 90-day expiry pushed back at most once a day. No cookies: the browser keeps the token in `localStorage`, the app in its private data | One mechanism for the browser and the app, with nothing cookie-specific (`Secure`, `SameSite`, CSRF). The frontend already fetches covers and files with `fetch`, so a header works everywhere. The cost: a script running in the page could read the token, and the only scripts there are Shelf's own (books render in foliate-js's sandboxed iframe). All auth data in one file is the owner's choice: one place to look and edit. Hashing the tokens costs nothing and keeps a copied file or backup from holding working tokens |
| Login rate limit | Per username, in memory: 5 wrong passwords in a row lock it for a minute | Nothing about the client's device is logged or kept (no IPs), and behind the same-host proxy every client has the proxy's address anyway. The cost: someone guessing can keep a user locked out |
| Users in `.shelf/` | Only the username, as a folder or file name. No passwords, hashes or sessions in the library | The library syncs to every device, so credentials in it would spread with it, and a local library has no server to check them against |
| Local-library user (planned) | The app asks which username to use, with no password and no role: every feature is available | The users, passwords and roles live in the server's `/data/data.json`, which a local library never sees, and credentials stay out of `.shelf/`. The username only picks whose progress files to use. Role-based features in the app apply to remote libraries, with the role the server reports. Locking the app (PIN, fingerprint) would be a separate feature |

### Users and `.shelf/` (partly built)

The server and the app share one `.shelf/` layout, so a library can be read through either with the same per-user data:

- A user is a **username**: lowercase `a–z`, `0–9`, `-`, `_`, 1 to 32 characters. It's used as a file name, like book IDs. Lowercase only, so it can't collide on a case-insensitive filesystem.
- **Shared:** book metadata and thumbnails (`books/<id>/`).
- **Per user:** progress (`books/<id>/progress/<user>.json`, built; the file also stores `user`), settings (`users/<user>/config.json`, built) and the last book opened (`users/<user>/state.json`, built). On the server the user is the one signed in; in the app's local libraries it's `default` until a user can be chosen.
- **Who the user is:** on the server, the logged-in session. In the app, the server decides for a remote library, from the bearer token. For a local library, the user is stored with the library entry in `AppData`. Using the same name on both gives the same progress.
- Per-user requests carry a `user` field in their proto message, so the `Library` trait and its impls get the user like any other argument, and HTTP routes and Tauri commands keep mirroring each other. The field is always filled in where the request enters, never trusted from the client: by the server's handlers (from the session) and by the app's `LocalLibrary` (from the app data). `BookLibrary` validates it.

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
| Metadata cache outside the library, keyed by path/size/mtime | Metadata in `.shelf/books/<id>/`, so it's extracted once and syncs. Startup skips hashing a copy whose mtime isn't newer than its location's `last_update_time` (the time this device last hashed it), at the cost of `metadata.json` differing per device |
| Progress at `.shelf/progress/<id>.json` with `fraction`, `chapter`, `updated`, `device` | `.shelf/books/<id>/progress/<user>.json` with the CFI and the user |
| Hand-written serde structs for on-disk files, preserving unknown fields | The generated proto types, via pbjson. Only the `config.json` files keep unknown fields; dropping them from the other files when they're rewritten is intended |
| Atomic writes staged in `.shelf/tmp/` | A hidden temp file directly in `.shelf/`, then a rename over the target (same filesystem, no extra folder for sync tools to ignore). Every `.shelf/` file |
| Reader preferences per device (localStorage / app store) | Per user, in the library (`.shelf/users/<user>/config.json`), except two per-device ones: reader zoom (`localStorage`, like browser zoom) and the app's interface scale (`AppData`, app only). The scale sets the root font size, since the UI is sized in rem: CSS `zoom` would also scale viewport units, and webview zoom doesn't work on Android. Other per-device preferences are on the roadmap |
| `ProgressChanged` events, resume toast, "jump to newer position" | Dropped: one device at a time |
| App in Server mode uses the frontend's `HttpBackend` with bearer tokens | The app's Rust `RemoteLibrary` talks to the server; the frontend always uses `tauriBackend` in the app |
| REST routes like `GET /api/books`, `PUT /api/books/{id}/progress` | `POST` routes with JSON bodies, one per `Library` method, mirrored by Tauri commands ([api.md](api.md)) |
| Watcher debounce ~500 ms, events `BooksChanged` / `ProgressChanged` | 5 s per path; a single `library-changed` event |
| A `LibraryFs` trait so Android SAF could be added later | A `LibraryStorage` trait (`crates/core/src/storage/`) with book paths relative to the root, for a local folder and S3. No SAF backend; only if Play Store distribution ever matters |
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
