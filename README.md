# Shelf

An EPUB reader whose library is just a folder of `.epub` files.

Shelf keeps everything it knows about your books (titles, covers, each reader's progress and settings) in a hidden `.shelf/` folder inside that library, so the library moves as one folder. There's no database and no import step: add a file to the folder and it shows up.

## Ways to run it

The same reader UI runs in three places:

| Runs as | Where the books are |
|---|---|
| **Web**: `shelf-server` in Docker, opened in a browser | On the server |
| **App, local library**: on Linux or Android | A folder on the device |
| **App, remote library**: on Linux or Android, connected to a `shelf-server` | On the server |

All three use the same on-disk format, so a library can move between a device and a server. A reader's progress, last book and settings can also be exported from one library and imported into another.

## What it does

- Browse the library by folder, with covers.
- Read in paginated or scrolled mode, with light/dark themes and margin sizes. Settings are stored per reader in the library, so they follow you to every device.
- Reopen a book on another device where you left off.
- On the server: accounts with roles (reader, contributor, librarian, admin, owner), sign-up requests an admin approves, and per-user folders and book filters.
- In the app: keep several named libraries, local or remote, and switch between them.
- *Rebuild index* when the library's metadata gets out of step with its files.

Shelf assumes one reader on one device at a time. Progress is read when a book opens; two devices reading at once don't update each other.

## Run the server

The image is published at `ghcr.io/psomani16k/shelf`. Mount your library at `/books` and a folder for the server's own data (users and sessions) at `/data`, outside the library:

```sh
docker run -d --name shelf -p 8080:8080 -v /path/to/books:/books -v /path/to/data:/data ghcr.io/psomani16k/shelf:latest
```

Or copy [`docker-compose.yml`](docker-compose.yml), set the volumes, and run `docker compose up -d`.

- The server runs as uid 1000 and writes `.shelf/` into the library and `data.json` into `/data`, so both folders must be writable by uid 1000.
- **Add the first user by hand.** On the first start the server creates an empty `/data/data.json`. Add an owner to it (no restart needed), e.g. `{ "users": { "me": { "password": "…", "role": "OWNER" } } }`, then sign in at http://localhost:8080. Others can ask for an account on the sign-in page, and an admin approves them in *Settings → Admin*.
- Passwords are stored in plain text, and tokens and passwords are sent with every request: put the server behind HTTPS (a reverse proxy or Tailscale) and don't reuse a password that matters.
- Logs are JSON lines on stdout: `docker logs shelf`.

More in [docs/server.md](docs/server.md).

## The app

There are no published app builds yet; build it from source by following [CONTRIBUTING.md](CONTRIBUTING.md). On first launch, pick a folder (local library, which asks who's reading) or enter a server address and sign in (remote library).

On Android, a local library needs *All files access*, which the app asks for. Android builds are sideloaded; Shelf isn't on the Play Store.

## Status

Shelf is a personal project in active development. It works end to end on the web, Linux and Android. Known gaps:

- The Linux app can be laggy on some GPU setups (a WebKitGTK rendering issue).
- EPUB only.

See [docs/known-issues.md](docs/known-issues.md) and [docs/roadmap.md](docs/roadmap.md).

## Contributing and docs

- [CONTRIBUTING.md](CONTRIBUTING.md): set up a development environment, run each part, and the checks to run before a pull request.
- [docs/](docs/README.md): how the code works, the on-disk format, the API, and why things are built the way they are.
