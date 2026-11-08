# Shelf

An EPUB reader whose library is just a folder of `.epub` files.

Shelf keeps everything it knows about your books (titles, covers, reading progress, settings) in a hidden `.shelf/` folder inside that library. Sync the folder between devices with whatever you already use (Syncthing, Nextcloud, a NAS share) and your place in every book comes along with it. There's no database and no import step: add a file to the folder and it shows up.

## Ways to run it

The same reader UI runs in three places:

| Runs as | Where the books are |
|---|---|
| **Web**: `shelf-server` in Docker, opened in a browser | On the server |
| **App, local library**: on Linux or Android | A folder on the device |
| **App, remote library**: on Linux or Android, connected to a `shelf-server` | On the server |

All three use the same on-disk format, so they mix: laptops reading a synced folder directly, and a phone talking to a server whose library is that same folder.

## What it does

- Browse the library by folder, with covers.
- Read in paginated or scrolled mode, with light/dark themes and margin sizes. Settings are stored in the library, so they follow it to every device.
- Reopen a book on another device where you left off.
- In the app: keep several named libraries, local or remote, and switch between them.
- *Rebuild index* when the library's metadata gets out of step with its files.

Shelf assumes one reader on one device at a time. Progress is read when a book opens; two devices reading at once don't update each other.

## Run the server

The image is published at `ghcr.io/psomani16k/shelf`. Mount your library at `/books`:

```sh
docker run -d --name shelf -p 8080:8080 -v /path/to/books:/books ghcr.io/psomani16k/shelf:latest
```

Then open http://localhost:8080. Or copy [`docker-compose.yml`](docker-compose.yml) next to your books, set the volume, and run `docker compose up -d`.

- The server runs as uid 1000 and writes `.shelf/` into the library, so that folder must be writable by uid 1000.
- **There's no authentication yet.** Run it only on a network you trust (your LAN, Tailscale) or behind a proxy that handles login.
- Logs are JSON lines on stdout: `docker logs shelf`.

More in [docs/server.md](docs/server.md).

## The app

There are no published app builds yet; build it from source by following [CONTRIBUTING.md](CONTRIBUTING.md). On first launch, pick a folder (local library) or enter a server address (remote library).

On Android, a local library needs *All files access*, which the app asks for. Android builds are sideloaded; Shelf isn't on the Play Store.

## Status

Shelf is a personal project in active development. It works end to end on the web, Linux and Android. Known gaps:

- No login on the server.
- The Linux app can be laggy on some GPU setups (a WebKitGTK rendering issue).
- EPUB only.

See [docs/known-issues.md](docs/known-issues.md) and [docs/roadmap.md](docs/roadmap.md).

## Contributing and docs

- [CONTRIBUTING.md](CONTRIBUTING.md): set up a development environment, run each part, and the checks to run before a pull request.
- [docs/](docs/README.md): how the code works, the on-disk format, the API, and why things are built the way they are.
