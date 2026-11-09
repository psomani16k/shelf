# Library format and syncing

A library is any folder. Shelf reads the `.epub` files in it (recursively) and writes its own data only into `<library>/.shelf/`. Nothing else in the library is ever written: no logs, caches or device settings.

## Scanning rules

- A book is any regular file whose extension is `epub`, case-insensitive.
- Hidden entries are skipped: any file or folder whose name starts with `.`, at any depth. This also skips `.shelf/`.
- Symlinks are skipped, both files and folders (`DirEntry::file_type` doesn't follow them), which also avoids loops.
- Folder listings in the UI are derived from book paths, so a folder that contains no books (at any depth) never appears.

Scanning is `LibraryStorage::all_book_file_paths` (`LocalLibraryStorage` in `crates/core/src/storage/local_storage.rs`), which returns paths relative to the library root; the watcher applies the same rule in `LibraryWatcher::is_relevant` (`crates/core/src/watcher.rs`).

## Book IDs

A book's ID is the **lowercase hex SHA-1 of the whole file's bytes** (`EbookExtractor::read`, `crates/core/src/ebook_ingest.rs`). Consequences:

- Moving or renaming a file keeps its ID, so progress follows it.
- Editing a file (new bytes) gives it a new ID; its old progress is dropped at the next startup sync.
- Two identical copies of a file share one ID and one `.shelf/books/<id>/` folder; the metadata's `path` points at whichever copy was synced last.
- IDs are 40 alphanumeric characters. `RemoteLibrary` relies on that to use them as cache file names safely.

## `.shelf/` layout

```
<library>/
  .shelf/
    books/
      <sha1>/
        metadata.json    BookMetadata
        thumbnail.jpeg   only if a cover image was found
        progress.json    BookProgress, only once the book has been read
    state.json           State: the last book opened
    config.json          Config: library settings (reader and UI), partially updated by set_config
```

All files are pretty-printed JSON using the **proto JSON shape**: snake_case keys, enums by name. The Rust types are the generated proto types (`shelf_proto::bookkeeping`, `::state`, `::config`) serialized with pbjson, which ignores unknown fields when reading. So only `config.json` (merged as raw JSON) keeps fields this build doesn't know; rewriting the other files drops them.

### `metadata.json` (`bookkeeperpb.BookMetadata`)

```json
{
  "path": "Sci-Fi/dune.epub",
  "title": "Dune",
  "authors": ["Frank Herbert"],
  "file_type": "EPUB",
  "id": "3f786850e387550fdab836ed7e6dc881de23001b",
  "last_update_time": "2026-10-26T04:45:00Z"
}
```

`path` is relative to the library root with `/` separators. `last_update_time` is when the metadata was last written after hashing the file (RFC 3339 UTC, whole seconds, from `shelf_utility::timeutil::Time`); it lets the startup sync skip hashing unchanged books. Title, authors and the thumbnail are extracted once; each later hash of the same file rewrites only `path`, `id` and `last_update_time`.

The extracted fields are deterministic, but `last_update_time` is each device's own clock, so two devices that both hash a book write different bytes (see [known-issues.md](known-issues.md#data-safety)).

### `thumbnail.jpeg`

The EPUB's cover image scaled down (never up, never cropped) to fit 400×600, re-encoded as JPEG quality 85.

The cover is found by `find_cover` (`crates/core/src/ebook_ingest.rs`), trying in order:

1. The declared cover (rbook's `cover_image`): the manifest item with `properties="cover-image"` (EPUB 3), else the item whose id the EPUB 2 `<meta name="cover" content="…">` names.
2. The manifest item with id `cover`: itself if it's an image, else the first image (`<img src>`, or SVG `<image href>` / `xlink:href`) on that page, resolved relative to the page. This comes before the next two because some books point both at a placeholder cover page put in front of the real one.
3. The EPUB 2 `<meta name="cover">`'s `content` read as a path instead of an id, matched against manifest `href`s. Some publishers write `content="Images/cover.jpg"`.
4. The first image on the landmarks/guide entry of type `cover`.

Names alone are never used to guess a cover, since books also have files like `backcover.jpg`. The `thumbnail written` debug log says which step found it (`found_by`: `declared`, `cover item`, `meta path` or `landmark page`). A book whose only cover is a declared placeholder gets the placeholder. A cover that can't be read or decoded (such as an SVG, or a format the `image` crate isn't built with: only JPEG, PNG, GIF and WebP are) is logged at `warn`, and the book is kept without a thumbnail; `book extracted` then lists `thumbnail` in `missing`.

### `progress.json` (`bookkeeperpb.BookProgress`)

```json
{ "file_type": "EPUB", "epub_progress": { "cfi": "epubcfi(/6/14!/4/2/10,/1:0,/1:120)" } }
```

### `state.json` (`statepb.State`)

```json
{ "last_book_id": "3f786850e387550fdab836ed7e6dc881de23001b" }
```

`set_state` reads the file into `State` first and only replaces `last_book_id`. That doesn't keep unknown fields: pbjson drops them on read, so a field written by a newer build is lost (the code comment in `book_library.rs` claims otherwise). `get_state` reports `last_book_id` as unset when that book is no longer in the library.

### `config.json` (`configpb.Config`)

```json
{
  "reader": { "flow": "PAGINATED", "theme": "DARK", "margins": { "side": "LARGE" } },
  "ui": { "theme": "SYSTEM" }
}
```

Every scalar/enum field in `config.proto` is `optional`, so a patch can tell "not sent" from the zero value. `set_config` (`LibraryStorage::update_config`, a default method in `crates/core/src/storage/mod.rs`) merges the patch into the stored **raw JSON** (objects merge key by key; anything else, lists included, replaces), so keys this build doesn't know survive. A mutex in `BookLibrary` serializes concurrent `set_config` calls. A missing or unparseable file reads as the default config (zero values: scrolled, reader matches UI, medium margins, system theme).

### Writes

`state.json` and `config.json` are written atomically: a hidden temp file `.<name>.tmp-<pid>-<nanos>` in the same folder, `fsync`, then rename (`write_atomic`, `crates/core/src/storage/local_storage.rs`). Leftover temp files are deleted when a library opens, right after the startup sync (`LocalLibraryStorage::clean_up_temp_files`). `metadata.json`, `progress.json` and `thumbnail.jpeg` are currently written with a plain `fs::write` (see [known-issues.md](known-issues.md)).

### S3 storage

`S3LibraryStorage` (`crates/core/src/storage/s3_storage.rs`, cargo feature `s3`, built on `object_store`) keeps the same layout under a key prefix in a bucket: books at `<prefix>/<path>`, data at `<prefix>/.shelf/…`, with the same scanning rules (dot-parts skipped, `.epub` case-insensitive). Credentials, region and endpoint come from the standard `AWS_*` env vars (`AWS_ENDPOINT` for MinIO or R2). Every write is a single PUT, which replaces an object whole, so there are no temp files, and `create_book_dir` and `clean_up_temp_files` do nothing. A book's mtime is its object's `last_modified`. The blocking trait methods wait on the tokio runtime the storage was created in, so they must run in `spawn_blocking`. It isn't wired into the server or app yet (see [known-issues.md](known-issues.md#s3-storage)).

## Startup sync

`sync_library` (`crates/core/src/routines/extractor.rs`) runs on every `BookLibrary::open`. It lists the folders, metadata and book files in one `spawn_blocking`, then syncs each book file in its own `spawn_blocking` job, two per CPU at a time, so one can use a core while another waits on the disk (`shelf_utility::routines::transform_concurrent_blocking`), and tallies the results once they're all in:

1. List every `.shelf/books/<id>` folder.
2. Load every `metadata.json`, keyed by `path`.
3. For every book file:
   - **Skip hashing** (`EbookExtractor::predict_id_change_using_modified_time`) when a `metadata.json` has this `path`, a non-empty `id`, and a `last_update_time` no earlier than the file's mtime (both compared to the whole second). The stored `id` is used as is.
   - Otherwise read and SHA-1 the whole file (`EbookExtractor::read`, which also loads `.shelf/books/<id>/metadata.json` if there is one), then `extract(force)`:
     - metadata exists and `force` is off: set `path`, `id` and `last_update_time` in it if they changed, and write it only then (the stamp is refreshed when the file's mtime has passed it, so a touched but identical file is hashed once, not on every start);
     - otherwise: create the folder, parse the EPUB with `rbook` from the bytes already read, write `metadata.json` (with `last_update_time`) and the thumbnail.
4. Delete every `.shelf/books/<id>/` folder whose ID no file produced. **This deletes that book's progress too.**
5. Log `library synced` with `books`, `hashed` (read and synced successfully), `skipped`, `failed` and `removed` counts.

A file that fails (for example half-copied) is logged at `warn` and retried on the next watcher event or startup. A file that was hashed but couldn't be extracted (an EPUB rbook can't parse) still counts as producing its ID, so its `.shelf/books/<id>/` folder and progress are kept; the folder is only created once the EPUB has parsed. Only a file that can't be read at all leaves its ID unproduced.

So only new and modified books are hashed; an unchanged library starts with one `stat` per book plus reading its `metadata.json`. Metadata written before `last_update_time` existed has no stamp, so the first start after upgrading hashes everything once and adds it.

**Rebuild index** (*Settings → Library*, `Library::rebuild_index`) runs the same sync with `force` set: every book is hashed regardless of `last_update_time` and fully re-extracted (title, authors, thumbnail and stamp), then the in-memory index is replaced with what's on disk. It fixes stale IDs (such as a same-path replacement with an older mtime).

After the sync, every `metadata.json` is loaded into an in-memory `Vec<BookMetadata>` behind a `tokio::sync::RwLock`. Browsing (`list_books`, `list_sub_directories`) and ID checks only read that list; they never touch the disk or hash anything.

## Watcher

`LibraryWatcher` (`crates/core/src/watcher.rs`) wraps `notify`'s recommended watcher, recursive on the library root:

- Only create, modify and remove events are considered.
- Paths are filtered **before** queuing (`LibraryWatcher::is_relevant`): visible `.epub` paths, folders, and paths that are already gone (a gone path can't be checked, and it may have been a folder) pass, below the root only. Existing non-book files and anything under a dot-folder are dropped, so writes in `.shelf/` never trigger it.
- Each path goes into an unbounded `tokio::sync::mpsc` channel.

`BookLibrary::receive_and_process_library_updates` (`crates/core/src/library/book_library.rs`) debounces with `shelf_utility::routines::Debounced`: each path fires **5 s after its last event**; a new event for the same path resets its timer. There is no 60 s upper bound yet ([roadmap](roadmap.md#library-core)). When a path fires, its sync job (`IndexSync::sync_path`, `crates/core/src/routines/index_sync.rs`) runs on tokio's blocking pool, at most one job per CPU (each job holds a whole book in memory while it runs):

- Folder exists → every `.epub` under it, at any depth, goes through the book cases below, one after another. A book that fails logs a `warn` and the rest carry on. This is how a renamed, moved or copied-in folder is picked up: inotify reports the folder, not the books inside it. On a rename, each book's hash matches its existing `.shelf/books/<id>/`, so only its `path` is rewritten.
- Path is gone and isn't an `.epub` → treated as a folder moved away or deleted: every entry whose `path` starts with `<path>/` is removed from memory.
- File exists and is unchanged (the same `predict_id_change_using_modified_time` check against its in-memory metadata) → nothing to do, nothing read; logged at `debug`, and `on_change()` isn't called.
- File exists and is new or modified → read and hash it, `extract(false)` (same routine as the startup sync), and put its metadata in memory, replacing entries with the same ID (a moved book only changes `path`) or the same `path` (an edited book's old ID). If the index already had exactly this entry, `on_change()` isn't called.
- File is gone → `EbookExtractor::read` scans every `metadata.json` for the ID that still has that `path`, and `extract` returns it with no metadata. Entries with that ID and that `path` are removed from memory, or, if no ID was found, entries with that `path`. Matching the path too matters for a move: its create job can run at the same time and may already have put the ID back under the new path. If nothing was removed, `on_change()` isn't called. The `.shelf/books/<id>/` folder stays until the next startup sync, because a move's remove event can arrive before its create event.

Otherwise `on_change()` is called. It doesn't look at event kinds (add, rename, …): it just looks at the disk.
