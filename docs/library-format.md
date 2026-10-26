# Library format and syncing

A library is any folder. Shelf reads the `.epub` files in it (recursively) and writes its own data only into `<library>/.shelf/`. Nothing else in the library is ever written: no logs, caches or device settings.

## Scanning rules

- A book is any regular file whose extension is `epub`, case-insensitive.
- Hidden entries are skipped: any file or folder whose name starts with `.`, at any depth. This also skips `.shelf/`.
- Symlinks are skipped, both files and folders (`DirEntry::file_type` doesn't follow them), which also avoids loops.
- Folder listings in the UI are derived from book paths, so a folder that contains no books (at any depth) never appears.

Scanning is `BookKeeper::all_book_file_paths` (`crates/core/src/bookkeeper.rs`); the watcher applies the same rule in `LibraryWatcher::is_relevant` (`crates/core/src/watcher.rs`).

## Book IDs

A book's ID is the **lowercase hex SHA-1 of the whole file's bytes** (`EbookExtractor::extract`, `crates/core/src/ebook_ingest.rs`). Consequences:

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
        thumbnail.jpeg   only if the EPUB declares a cover image
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

`path` is relative to the library root with `/` separators. `last_update_time` is when the metadata was last written after hashing the file (RFC 3339 UTC, whole seconds, from `crate::timeutil::Time`); it lets the startup sync skip hashing unchanged books. Title, authors and the thumbnail are extracted once; each later hash of the same file rewrites only `path`, `id` and `last_update_time`.

The extracted fields are deterministic, but `last_update_time` is each device's own clock, so two devices that both hash a book write different bytes (see [known-issues.md](known-issues.md#data-safety)).

### `thumbnail.jpeg`

The EPUB's cover image scaled down (never up, never cropped) to fit 400×600, re-encoded as JPEG quality 85.

### `progress.json` (`bookkeeperpb.BookProgress`)

```json
{ "file_type": "EPUB", "epub_progress": { "cfi": "epubcfi(/6/14!/4/2/10,/1:0,/1:120)" } }
```

### `state.json` (`statepb.State`)

```json
{ "last_book_id": "3f786850e387550fdab836ed7e6dc881de23001b" }
```

`set_state` reads the file into `State` first and only replaces `last_book_id`. That doesn't keep unknown fields: pbjson drops them on read, so a field written by a newer build is lost (the code comment in `library.rs` claims otherwise). `get_state` reports `last_book_id` as unset when that book is no longer in the library.

### `config.json` (`configpb.Config`)

```json
{
  "reader": { "flow": "PAGINATED", "theme": "DARK", "margins": { "side": "LARGE" } },
  "ui": { "theme": "SYSTEM" }
}
```

Every scalar/enum field in `config.proto` is `optional`, so a patch can tell "not sent" from the zero value. `set_config` merges the patch into the stored **raw JSON** (objects merge key by key; anything else, lists included, replaces), so keys this build doesn't know survive. A mutex in `BookLibrary` serializes concurrent `set_config` calls. A missing or unparseable file reads as the default config (zero values: scrolled, reader matches UI, medium margins, system theme).

### Writes

`state.json` and `config.json` are written atomically: a hidden temp file `.<name>.tmp-<pid>-<nanos>` in the same folder, `fsync`, then rename (`write_atomic`, `crates/core/src/state.rs`). Leftover temp files are deleted when a library opens. `metadata.json`, `progress.json` and `thumbnail.jpeg` are currently written with a plain `fs::write` (see [known-issues.md](known-issues.md)).

## Startup sync

`sync_library` (`crates/core/src/routines/extractor.rs`) runs on every `BookLibrary::open`, in a blocking thread, with rayon over all files:

1. List every `.shelf/books/<id>` folder.
2. Load every `metadata.json`, keyed by `path`.
3. For every book file:
   - **Skip hashing** (`EbookExtractor::unchanged_id`) when a `metadata.json` has this `path`, a non-empty `id`, and a `last_update_time` no earlier than the file's mtime (both compared to the whole second). The stored `id` is used as is.
   - Otherwise read and SHA-1 the whole file, then
     - if `.shelf/books/<id>/` exists: rewrite `path`, `id` and `last_update_time` in its `metadata.json` (always, so a touched but identical file is hashed once, not on every start);
     - otherwise: create the folder, parse the EPUB with `rbook`, write `metadata.json` (with `last_update_time`) and the thumbnail.
4. Delete every `.shelf/books/<id>/` folder whose ID no file produced. **This deletes that book's progress too.**
5. Log `library synced` with `books`, `hashed`, `skipped`, `failed` and `removed` counts.

A file that fails (for example half-copied) is logged at `warn` and retried on the next watcher event or startup.

So only new and modified books are hashed; an unchanged library starts with one `stat` per book plus reading its `metadata.json`. Metadata written before `last_update_time` existed has no stamp, so the first start after upgrading hashes everything once and adds it.

**Rebuild index** (*Settings → Library*, `Library::rebuild_index`) runs the same sync with `force` set: every book is hashed regardless of `last_update_time`, then the in-memory index is replaced with what's on disk. It fixes stale IDs (such as a same-path replacement with an older mtime) and re-stamps every book. Title, authors and thumbnails of known books aren't re-extracted.

After the sync, every `metadata.json` is loaded into an in-memory `Vec<BookMetadata>` behind a `tokio::sync::RwLock`. Browsing (`list_books`, `list_sub_directories`) and ID checks only read that list; they never touch the disk or hash anything.

## Watcher

`LibraryWatcher` (`crates/core/src/watcher.rs`) wraps `notify`'s recommended watcher, recursive on the library root:

- Only create, modify and remove events are considered.
- Paths are filtered **before** queuing: only visible `.epub` paths pass, so writes in `.shelf/` never trigger it.
- Each path goes into an unbounded `tokio::sync::mpsc` channel.

The `extractor` task (`routines/extractor.rs`) debounces with a `tokio_util::time::DelayQueue`: each path fires **5 s after its last event**; a new event for the same path resets its timer. There is no 60 s upper bound yet ([roadmap](roadmap.md#library-core)). When a path fires, `sync_path` runs on the rayon pool:

- File exists and is unchanged (the same `unchanged_id` check against its in-memory metadata) → nothing to do; logged at `debug`, and `on_change()` isn't called.
- File exists and is new or modified → extract it (same routine as the startup sync) and upsert its metadata in memory, matched by ID, so a moved book only changes `path`.
- File is gone → remove entries with that `path` from memory. The `.shelf/books/<id>/` folder stays until the next startup sync, because a move's remove event can arrive before its create event.

Otherwise `on_change()` is called. It doesn't look at event kinds (add, rename, …): it just looks at the disk.
