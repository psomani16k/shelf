# Shelf docs

Shelf is an EPUB reader where the library is a plain folder of `.epub` files. Shelf keeps its own data (metadata, cover thumbnails, reading progress, settings) in a hidden `.shelf/` folder inside that library, so the data travels with the books when the folder is synced between devices (Syncthing, Nextcloud, a NAS share, …).

It runs in three ways, all with the same React UI:

| Runs as | Talks to | Where the library is |
|---|---|---|
| **Web**: `shelf-server` in Docker, opened in a browser | HTTP `/api/*` on the same origin | `/books` inside the container |
| **App, local library**: Tauri on Linux or Android | Tauri IPC → Rust in the app | A folder on the device |
| **App, remote library**: Tauri on Linux or Android | Tauri IPC → Rust in the app → HTTP to a `shelf-server` | On the server |

## Where to read next

| Doc | Read it when you need… |
|---|---|
| [decisions.md](decisions.md) | Goals, non-goals, the settled technical choices and why, and where the original plan changed |
| [roadmap.md](roadmap.md) | Everything planned but not built yet |
| [architecture.md](architecture.md) | The big picture: crates, the `Library` trait every backend implements, how a request flows in each mode |
| [library-format.md](library-format.md) | The `.shelf/` on-disk format, book IDs, the startup sync and the watcher |
| [api.md](api.md) | Wire conventions, every HTTP route and Tauri command, error shape, events |
| [server.md](server.md) | `shelf-server`: routes, logging, Docker image, publishing to ghcr, compose |
| [app.md](app.md) | The Tauri crate: state, library sources, commands, the Android plugin, Android builds and signing |
| [frontend.md](frontend.md) | The React app: the `Backend` interface, `IS_TAURI`, screens, reader, settings, safe-area insets |
| [development.md](development.md) | Setting up, running, testing, conventions, and how the user likes to work |
| [known-issues.md](known-issues.md) | Bugs, risks and cleanups in what's built |

## Status (2026-10-26)

Working end to end:
- Web UI served by `shelf-server`, image published by hand to `ghcr.io/psomani16k/shelf`.
- Tauri app with a library picker (local folder or server URL), the same Home/Reader/Settings UI as the web, plus app-only named libraries: switch in the library slideover, add and remove in *Settings → Library*. *Rebuild index* there works in both.
- Android release APK (signed with a self-made keystore), with storage permission, a native folder picker and system-bar padding.

Not built yet: authentication and more; see [roadmap.md](roadmap.md). On Linux the Tauri app is laggy on the developer's machine (WebKitGTK on hybrid NVIDIA/AMD under Wayland), so testing has moved to Android. See [known-issues.md](known-issues.md).

## Ground rules for agents

`CLAUDE.md` at the repo root holds the project rules (code style, logging rules, what never to log, dev environment) and a summary of how the code works. Read it first. These docs go into more detail. Both describe the code as it is; planned work is in [roadmap.md](roadmap.md) and problems in [known-issues.md](known-issues.md). When you change behaviour, update the docs that describe it.
