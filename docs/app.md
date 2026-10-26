# The Tauri app

`app/` is a Tauri 2 app for Linux and Android. The Rust side holds the open library; the WebView runs the same React frontend as the web build. Bundle identifier: `com.psomani16k.shelf` (changing it makes Android treat it as a different app).

## Rust layout (`app/src`)

| File | Role |
|---|---|
| `main.rs` | Desktop entry; calls `app_lib::run()` |
| `lib.rs` | Builder: plugins, `invoke_handler` with every command, `setup` (`AppData`, then logging, then `AppState`) |
| `logging/mod.rs` | `init` (the `tracing` subscriber) and the log commands |
| `logging/files.rs` | `LogFiles`: one log file per app start, pruned to the newest N |
| `state.rs` | `AppState`: the stored library list and the currently open library (opened by name) |
| `app_data.rs` | `AppData`: the library list, the current library's name and the log-file count, persisted with tauri-plugin-store (`app_data.json`); `validate_name`, `validate_log_files` |
| `commands.rs` | Library management commands and the `Library` passthroughs |
| `error.rs` | `CommandError { status, message }`, the only place command errors are logged; `CommandError::invalid` for 400s |
| `native.rs` | The `shelf` plugin: storage permission, folder picker, system insets (Android) |
| `libsrc/mod.rs` | `LibrarySource` (what's stored, with `name()`) and `AnyLibrary` (what's open) |
| `libsrc/local.rs` | `LocalLibraryBuilder` / `LocalLibrary` over `shelf_core::BookLibrary` |
| `libsrc/remote.rs` | `RemoteLibraryBuilder` / `RemoteLibrary`, an HTTP client for `shelf-server` |
| `libsrc/app_library.rs` | `AppLibrary` trait: `get_name`, `get_builder` |

Plugins registered in `lib.rs`:

- `tauri-plugin-http`: Rust uses its `reqwest` re-export.
- `tauri-plugin-dialog`: the desktop folder picker and the *Save logs* picker.
- `tauri-plugin-store`: app data.
- `tauri-plugin-fs`: Rust writes saved logs through it, so Android `content://` URIs work.
- `tauri-plugin-clipboard-manager`: *Copy logs*, called from Rust.
- `tauri-plugin-log` with `skip_logger()`: only its `log` command, for frontend logs.
- `native::init()`: Shelf's own.

`capabilities/default.json` grants `core:default`, `dialog:default`, `http:default` and `log:default`. The app's own commands in `invoke_handler` need no capability entry, and neither do plugins used only from Rust.

The npm package and the Rust crate of a plugin must be on the same major/minor version, or `cargo tauri dev`/`build` refuses to run. That's why `tauri-plugin-log` is pinned to `~2.9` on both sides: the npm package has no 2.10.

Tauri commands are like Go HTTP handlers registered on a mux: `#[tauri::command]` functions, with `State<'_, AppState>` injected the way you'd close over a dependency, and JSON args/results serialized by serde.

## State and library lifecycle

```rust
pub struct AppState {
    pub data: tokio::sync::Mutex<AppData>,            // the stored libraries, the current name, the log-file count
    library: tokio::sync::Mutex<Option<Arc<AnyLibrary>>>, // the open one
}
```

- `setup` loads `AppData` from the store first, starts logging with its log-file count, logs anything `AppData::load` had to fall back on (it ran before logging existed), then wraps it in `AppState::new`. No library is open yet.
- Libraries are identified by **name**: required, trimmed, unique ignoring case (`app_data::validate_name`). A bad name, path or URL rejects with a 400 `CommandError` (logged at debug), not a 500.
- `add_local_library(name, path)` / `add_remote_library(name, baseUrl)` add the library and make it the **current** one (the frontend then switches to it).
- `select_library(name)` makes a library current without opening it; the frontend's open query does that.
- `open_library(name)` → `AppState::open`: clones that `LibrarySource`, builds it, and replaces the open library. The `on_change` it passes emits the `library-changed` event. For a local library, building runs `BookLibrary::open`, so the command resolves only after the startup sync. Dropping the previous `Arc` stops its watcher.
- Passthrough commands get the library with `state.library()`, which errors with "no library is open" before `open_library`.
- `remove_library(name)` forgets the library on this device (its folder or server is untouched), clears `current` if it was current, and closes it if it's the open one (`AppState::open_name`).

## Stored data

`AppData` lives in the app's private data directory, never in a library:

- Store file `app_data.json`, keys `libraries` and `current` (the name of the library to open on start), via tauri-plugin-store (relative paths resolve to the app data dir). On Linux: `~/.local/share/com.psomani16k.shelf/app_data.json`. On Android: the app's internal storage.
- Each entry is a serde externally tagged `LibrarySource`:

```json
{
  "libraries": [
    { "Local": { "root": "/storage/emulated/0/Books", "name": "Books" } },
    { "Remote": { "base_url": "http://nas:8080", "cache_dir": "/home/me/.cache/com.psomani16k.shelf/remote/nas-8080", "name": "NAS" } }
  ],
  "current": "NAS"
}
```

A missing or unreadable list loads as empty (with a warning) rather than blocking the app. Libraries from before names were required already had one (the folder name or host), so they load unchanged. Two remote libraries on the same server share one download cache, since it's keyed by content hash.

## Local libraries

`LocalLibrary` wraps `BookLibrary` (see [library-format.md](library-format.md)). Added with `add_local_library(path)`: the path is canonicalized and must be a folder; the name is the folder's name.

## Remote libraries

`RemoteLibrary` (`libsrc/remote.rs`) implements `Library` over a `shelf-server`:

- `add_remote_library(baseUrl)`: `http://` is prepended when there's no `://`; only http/https are accepted; name = host; cache dir = `<app cache dir>/remote/<host>-<port>` (Linux: `~/.cache/com.psomani16k.shelf/remote/...`). Nothing is contacted when adding or opening; the first request is what reaches the server.
- `api_call(req, path)`: POSTs JSON to `{base_url}/api{path}`. A non-2xx response becomes an error carrying `HttpStatus`, with the server's `ApiError.message` in the context; `to_library_error` maps a 404 to `LibraryError::NotFound`. An empty body (204) decodes as `null` → `()`.
- `get_cover` / `get_book`: download `GET /api/books/{id}/{cover|file}` once into `<cache>/{cover|file}/<id>.{jpeg|epub}` (via a `.part-<pid>` temp file and rename), then return the cached path. IDs are content hashes, so a cached file never goes stale. The ID must be alphanumeric, so a server can't make the app write outside the cache.
- `list_books` / `list_sub_directories` can't fail in the trait, so errors are logged at `warn` and an empty listing is returned. An unreachable server therefore looks like an empty library.
- The client has a 10 s connect timeout and no overall timeout. The cache is never evicted and isn't deleted when the library is removed.

## Logging

`logging::init` runs in `setup` right after `AppData` is loaded (it needs the log-file count) and installs a `tracing-subscriber` registry, in both debug and release builds:

- **Levels:** release `info`; debug `debug`, with `tauri`, `wry`, `reqwest`, `hyper_util` and `notify` at `info`. `RUST_LOG` overrides either, as on the server.
- **File layer:** JSON lines with the same flags as the server (`flatten_event`, `with_current_span`, no span list), so keys match: `timestamp`, `level`, `target`, `message`, and event fields at the top level. Written to `app_log_dir()` (Linux: `~/.local/share/com.psomani16k.shelf/logs/`).
- **Console layer:** plain text to stdout on desktop. On Android it goes to logcat through `tracing-logcat`, tag `Shelf`, without time or level, since logcat adds both: `adb logcat -s Shelf`. Errors from the Kotlin plugin show under the `Tauri` tag.
- **`log` records** (tauri, wry, and the frontend through `tauri-plugin-log`'s command) become `tracing` events through the `LogTracer` that `try_init` installs. Frontend lines have targets starting with `webview`, and their fields are part of the message text (`tracing-log` drops `log` key-values).
- **Panics** are logged at `error` by a panic hook before the default one runs.

**Files** (`LogFiles`):
- **One file per app start**, named by the start time (`Time`, whole seconds, UTC): `shelf-2026-10-26T04:45:00Z.log`. Files are never renamed, and names sort by time. There's no size cap: a run's file grows for as long as the app runs. A restart within the same second carries on in that second's file.
- N, the number of files kept with the current one included, is a per-device setting: the `log_files` field of `AppData` (key `log_files` in `app_data.json`), default 5, any whole number from 1 to 50 (`validate_log_files`). An unreadable or out-of-range stored value falls back to 5, with a warning.
- **Pruning:** after creating this run's file, `open` deletes the oldest files until N are left. The current file is never deleted, even if the clock went back and it isn't the newest. Files in the folder that don't match `shelf-<time>.log` are ignored, including the `shelf.log` / `shelf.1.log` files from the first version of this code.
- Changing N takes effect at once: `set_log_files` saves it through `AppState`'s `AppData`, then `set_keep` prunes to the new count.
- The writer uses blocking `std::fs`, because `tracing` writers are synchronous; it's an exception to "tokio everywhere". Write errors go to `eprintln!` and the line is dropped, since logging them would loop.

**Commands** (`logging/mod.rs`; see [api.md](api.md#logs)): `log_settings`, `set_log_files`, `list_log_files` (each file's name, size, start time and last write, newest first), `copy_logs` (the last 256 KiB, starting at a whole line, onto the clipboard), `save_logs` (every file oldest first into one) and `save_log_file` (one file, by a name from the list; anything that isn't a `shelf-<time>.log` name is refused with a 400, so it can't reach outside the log folder). Both saves use the dialog plugin's save picker (the suggested name has its colons removed, which Android's shared storage doesn't allow), read the logs after it closes, and write through `tauri-plugin-fs`. The UI is *Settings → Developer options → Logs* ([frontend.md](frontend.md#settings-and-appearance)).

## Android

### Project files

`app/gen/android` is generated by `cargo tauri android init` but committed and hand-edited. Hand-made changes:

| File | Change |
|---|---|
| `app/src/main/java/com/psomani16k/shelf/ShelfPlugin.kt` | Shelf's native plugin (below) |
| `app/src/main/java/com/psomani16k/shelf/MainActivity.kt` | `enableEdgeToEdge()` before `super.onCreate` |
| `app/src/main/AndroidManifest.xml` | `READ_EXTERNAL_STORAGE` (maxSdkVersion 32), `MANAGE_EXTERNAL_STORAGE`, `requestLegacyExternalStorage="true"` |
| `app/build.gradle.kts` | Release signing from `keystore.properties` |

SDK levels: `minSdk 24`, `compileSdk`/`targetSdk 37`. Debug builds get the application ID suffix `.debug`, so a debug and a release install are separate apps with separate data. `usesCleartextTraffic` is `false` in release; that policy applies to Java/WebView networking, not to Rust's `reqwest`, which is what talks to servers.

### The `shelf` plugin

`native.rs` registers the Kotlin class `ShelfPlugin` on Android (`register_android_plugin("com.psomani16k.shelf", "ShelfPlugin")`) and exposes four commands (see [api.md](api.md)). Each Android call goes through `PluginHandle::run_mobile_plugin`, which blocks until Kotlin resolves (for a picker, until the user is done), so it runs in `spawn_blocking`. On other platforms the commands return desktop answers without native code.

`ShelfPlugin.kt`:

- **Storage.** A library is a plain folder, so the app needs to read arbitrary files. On Android 11+ (API 30+) that's *All files access* (`MANAGE_EXTERNAL_STORAGE`), granted only on a system settings page: `requestStorage` opens `ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION` and reports `Environment.isExternalStorageManager()` when the user returns. Below 30 it requests `READ_EXTERNAL_STORAGE` through Tauri's permission alias `storage`.
- **Folder picker.** `pickFolder` launches `ACTION_OPEN_DOCUMENT_TREE` and maps the tree URI to a filesystem path: `primary:Books/Sci` → `/storage/emulated/0/Books/Sci`, `1234-ABCD:Books` → `/storage/1234-ABCD/Books`. Only the `com.android.externalstorage.documents` provider is supported (not Drive, Downloads shortcuts, etc.); others reject with a message. The tauri-plugin-dialog folder picker doesn't work on Android, which is why this exists.
- **Insets.** Because the app draws edge to edge, the WebView sits under the status and navigation bars. `systemInsets` returns `systemBars() | displayCutout()` insets divided by display density, i.e. in CSS pixels. The frontend turns them into CSS variables (see [frontend.md](frontend.md)).

### Building

Dev, with the phone on USB (`adb devices`):

```fish
cd app; and cargo tauri android dev
```

Release APK for arm64 phones:

```fish
cd app; and cargo tauri android build --apk --target aarch64
adb install -r gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

The version comes from `tauri.conf.json` (`0.1.0` → versionCode `1000`, via the generated `tauri.properties`).

### Icons

The icon is three off-white book spines and a leaning amber one on a shelf, on the UI's dark background (`#171717`). The sources are SVGs in `app/icons/source/`:

| File | Used for |
|---|---|
| `icon.svg` | Desktop icons, Windows tiles, and Android's fallback icon (the drawing on a rounded tile) |
| `android-fg.svg`, `android-bg.svg` | Android adaptive icon layers. The launcher shows only the middle 72 of 108 units and crops it to its own shape, so the drawing is scaled to 2/3 of the canvas |
| `android-monochrome.svg` | Android 13+ themed icons (white silhouette; the launcher recolours it) |
| `icon-manifest.json` | Ties them together for `cargo tauri icon`; `android_fg_scale` sizes the drawing in the Android 7 fallback icon |

`frontend/public/favicon.svg` is the same drawing cropped tighter so it reads at 16 px. To regenerate after changing a source:

```fish
cd app; and cargo tauri icon icons/source/icon-manifest.json; and rm -r icons/ios
```

It writes `app/icons/*` and, because `gen/android` exists, the Android `mipmap-*` folders and `mipmap-anydpi-v26/ic_launcher.xml`. The `ios/` folder it also creates isn't needed.

### Release signing

Android only installs signed APKs. Shelf isn't on the Play Store, so a self-made keystore is enough; the certificate details entered in `keytool` are arbitrary.

- Keystore: `~/.android/shelf-release.jks`, alias `shelf`, created with
  `keytool -genkey -v -keystore ~/.android/shelf-release.jks -keyalg RSA -keysize 2048 -validity 10000 -alias shelf`.
- `app/gen/android/keystore.properties` (git-ignored) holds `storeFile`, `storePassword`, `keyAlias`, `keyPassword`. `build.gradle.kts` loads it; without the file, release builds are unsigned and won't install.
- **Never print, log or commit the passwords or the keystore.**
- Updates must be signed with the same key. Losing the keystore means uninstalling and reinstalling, which loses the app's private data (the stored library list), not the library itself.

## Linux

```fish
cd app; and cargo tauri dev
```

`tauri.conf.json` runs `pnpm dev` (port 5173) from `../frontend` before starting, and `pnpm build` before bundling. On the developer's machine (Fedora, Wayland/niri, hybrid NVIDIA + AMD GPUs, webkit2gtk 2.54) the app is laggy and slow to show its first frame, in both debug and release. The backend isn't the cause (the library opens in under a second); it's WebKitGTK's rendering path. Environment switches tried without an acceptable result: `WEBKIT_DISABLE_DMABUF_RENDERER`, `WEBKIT_DISABLE_COMPOSITING_MODE`, `WEBKIT_FORCE_VBLANK_TIMER`, `WEBKIT_WEB_RENDER_DEVICE_FILE`, `__NV_DISABLE_EXPLICIT_SYNC`.
