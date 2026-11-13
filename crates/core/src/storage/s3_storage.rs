use std::{
    collections::HashSet,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use object_store::{
    GetOptions, ObjectMeta, ObjectStore, PutOptions, PutPayload, aws::AmazonS3Builder,
    path::Path as Key,
};
use shelf_proto::bookkeeping::{BookMetadata, BookProgress};
use shelf_utility::timeutil::Time;
use tokio::runtime::Handle;

use super::{FileContent, LibraryStorage, is_epub};

/// A library under a prefix in an S3 bucket (or any object store), laid out like a local one:
/// books at `<prefix>/<path>`, data at `<prefix>/.shelf/`. Each PUT replaces an object whole, so
/// there are no temp files. The blocking methods wait on the tokio runtime the storage was made
/// in, so they must run in `spawn_blocking`: on a runtime worker thread they panic.
#[derive(Debug, Clone)]
pub struct S3LibraryStorage {
    store: Arc<dyn ObjectStore>,
    prefix: Key,
    runtime: Handle,
}

impl S3LibraryStorage {
    /// Credentials, region and endpoint (for MinIO or R2) come from the standard `AWS_*` env
    /// vars. Must run inside tokio.
    pub fn new(bucket: &str, prefix: &str) -> Result<Self> {
        let store = AmazonS3Builder::from_env()
            .with_bucket_name(bucket)
            .build()
            .with_context(|| format!("couldn't set up the S3 bucket {bucket}"))?;
        Self::with_store(Arc::new(store), prefix)
    }

    /// A library in any object store, such as `InMemory` in tests. Must run inside tokio.
    pub fn with_store(store: Arc<dyn ObjectStore>, prefix: &str) -> Result<Self> {
        Ok(Self {
            store,
            prefix: Key::parse(prefix)?,
            runtime: Handle::try_current().context("S3 storage needs a tokio runtime")?,
        })
    }

    fn block_on<F: Future>(&self, f: F) -> F::Output {
        self.runtime.block_on(f)
    }

    /// The key of `path`, relative to the prefix.
    fn key(&self, path: &str) -> Result<Key> {
        Ok(Key::parse(format!("{}/{path}", self.prefix))?)
    }

    fn book_key(&self, path: &Path) -> Result<Key> {
        self.key(path.to_str().context("book path isn't UTF-8")?)
    }

    fn book_data_key(&self, hash: &str, file: &str) -> Result<Key> {
        self.key(&format!(".shelf/books/{hash}/{file}"))
    }

    /// The library path of `key`, or `None` if it's outside the prefix.
    fn library_path(&self, key: &Key) -> Option<PathBuf> {
        let parts: Vec<_> = key.prefix_match(&self.prefix)?.collect();
        let parts: Vec<&str> = parts.iter().map(|p| p.as_ref()).collect();
        Some(parts.join("/").into())
    }

    /// `None` if there's no such object.
    async fn get(&self, key: &Key) -> Result<Option<Bytes>> {
        match self.store.get_opts(key, GetOptions::default()).await {
            Ok(got) => Ok(Some(got.bytes().await?)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn put(&self, key: &Key, payload: impl Into<PutPayload>) -> Result<()> {
        self.store
            .put_opts(key, payload.into(), PutOptions::default())
            .await?;
        Ok(())
    }

    async fn list(&self, prefix: &Key) -> Result<Vec<ObjectMeta>> {
        Ok(self.store.list(Some(prefix)).try_collect().await?)
    }
}

impl LibraryStorage for S3LibraryStorage {
    fn all_book_file_paths(&self) -> Result<HashSet<PathBuf>> {
        self.book_file_paths_under(Path::new(""))
    }

    fn book_file_paths_under(&self, dir: &Path) -> Result<HashSet<PathBuf>> {
        let objects = self.block_on(self.list(&self.book_key(dir)?))?;
        Ok(objects
            .iter()
            .filter_map(|object| self.library_path(&object.location))
            .filter(|path| {
                // Same rule as a local scan; this also skips `.shelf/`.
                is_epub(path)
                    && !path
                        .iter()
                        .any(|part| part.to_string_lossy().starts_with('.'))
            })
            .collect())
    }

    fn is_folder(&self, path: &Path) -> Result<bool> {
        let key = self.book_key(path)?;
        let listing = self.block_on(self.store.list_with_delimiter(Some(&key)))?;
        Ok(!listing.objects.is_empty() || !listing.common_prefixes.is_empty())
    }

    fn sub_folders(&self, dir: &Path) -> Result<Vec<String>> {
        let key = self.book_key(dir)?;
        let listing = self.block_on(self.store.list_with_delimiter(Some(&key)))?;
        Ok(listing
            .common_prefixes
            .iter()
            .filter_map(|prefix| prefix.filename().map(str::to_owned))
            .filter(|name| !name.starts_with('.'))
            .collect())
    }

    fn add_book(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let key = self.book_key(path)?;
        self.block_on(self.put(&key, Bytes::copy_from_slice(bytes)))
    }

    /// Object stores have no empty folders, so this leaves a hidden marker the scanner skips.
    fn create_folder(&self, path: &Path) -> Result<()> {
        let key = self.book_key(&path.join(".keep"))?;
        self.block_on(self.put(&key, Bytes::new()))
    }

    fn delete_book_file(&self, path: &Path) -> Result<()> {
        let key = self.book_key(path)?;
        let keys = futures::stream::once(async move { Ok(key) }).boxed();
        self.block_on(self.store.delete_stream(keys).try_collect::<Vec<_>>())?;
        Ok(())
    }

    fn delete_folder(&self, path: &Path) -> Result<()> {
        let dir = self.book_key(path)?;
        let locations = self.store.list(Some(&dir)).map_ok(|o| o.location).boxed();
        self.block_on(self.store.delete_stream(locations).try_collect::<Vec<_>>())?;
        Ok(())
    }

    fn book_modified(&self, path: &Path) -> Result<Option<Time>> {
        let key = self.book_key(path)?;
        let head = GetOptions::new().with_head(true);
        match self.block_on(self.store.get_opts(&key, head)) {
            Ok(got) => Ok(Some(Time::from(SystemTime::from(got.meta.last_modified)))),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn read_book(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let book = self.block_on(self.get(&self.book_key(path)?))?;
        Ok(book.map(Vec::from))
    }

    async fn book_file(&self, path: &Path) -> Result<Option<FileContent>> {
        let book = self.get(&self.book_key(path)?).await?;
        Ok(book.map(FileContent::Bytes))
    }

    fn all_book_hashes_in_books_dir(&self) -> Result<HashSet<String>> {
        let books = self.key(".shelf/books")?;
        let listing = self.block_on(self.store.list_with_delimiter(Some(&books)))?;
        Ok(listing
            .common_prefixes
            .iter()
            .filter_map(|dir| dir.filename())
            .map(String::from)
            .collect())
    }

    fn book_metadata(&self, hash: &str) -> Result<Option<BookMetadata>> {
        let key = self.book_data_key(hash, "metadata.json")?;
        match self.block_on(self.get(&key))? {
            Some(json) => Ok(Some(serde_json::from_slice(&json)?)),
            None => Ok(None),
        }
    }

    fn set_book_metadata(&self, hash: &str, metadata: &BookMetadata) -> Result<()> {
        let key = self.book_data_key(hash, "metadata.json")?;
        self.block_on(self.put(&key, serde_json::to_string_pretty(metadata)?))
    }

    /// Object stores have no folders.
    fn create_book_dir(&self, _hash: &str) -> Result<()> {
        Ok(())
    }

    fn book_progress(&self, hash: &str, user: &str) -> Result<Option<BookProgress>> {
        let key = self.book_data_key(hash, &format!("progress/{user}.json"))?;
        match self.block_on(self.get(&key))? {
            Some(json) => Ok(serde_json::from_slice(&json)?),
            None => Ok(None),
        }
    }

    fn set_book_progress(&self, hash: &str, user: &str, progress: BookProgress) -> Result<()> {
        let key = self.book_data_key(hash, &format!("progress/{user}.json"))?;
        self.block_on(self.put(&key, serde_json::to_string_pretty(&progress)?))
    }

    async fn book_thumbnail(&self, hash: &str) -> Result<Option<FileContent>> {
        let thumbnail = self.get(&self.book_data_key(hash, "thumbnail.jpeg")?).await?;
        Ok(thumbnail.map(FileContent::Bytes))
    }

    fn set_book_thumbnail(&self, hash: &str, thumbnail: &[u8]) -> Result<()> {
        let key = self.book_data_key(hash, "thumbnail.jpeg")?;
        self.block_on(self.put(&key, Bytes::copy_from_slice(thumbnail)))
    }

    fn clean_up_books(&self, hashes: HashSet<String>) -> Result<usize> {
        let mut cleaned = 0;
        for hash in hashes {
            let dir = self.key(&format!(".shelf/books/{hash}"))?;
            let stale = [
                self.book_data_key(&hash, "metadata.json")?,
                self.book_data_key(&hash, "thumbnail.jpeg")?,
            ];
            let listed = self.store.list(Some(&dir)).map_ok(|o| o.location);
            // Like a local delete: one failed book doesn't stop the rest.
            let Ok(listed) = self.block_on(listed.try_collect::<Vec<_>>()) else {
                continue;
            };
            let mut deleted = false;
            for key in listed.iter().filter(|k| stale.contains(k)) {
                deleted |= self.block_on(self.store.delete(key)).is_ok();
            }
            cleaned += usize::from(deleted);
        }
        Ok(cleaned)
    }

    fn clean_up_progress(&self, keep: &HashSet<String>) -> Result<usize> {
        let mut deleted = 0;
        for hash in self.all_book_hashes_in_books_dir()? {
            if keep.contains(&hash) {
                continue;
            }
            let dir = self.key(&format!(".shelf/books/{hash}"))?;
            let metadata = self.book_data_key(&hash, "metadata.json")?;
            let listed = self.store.list(Some(&dir)).map_ok(|o| o.location);
            let Ok(listed) = self.block_on(listed.try_collect::<Vec<_>>()) else {
                continue;
            };
            if listed.contains(&metadata) {
                continue;
            }
            let keys = futures::stream::iter(listed.into_iter().map(Ok)).boxed();
            let removed = self.store.delete_stream(keys).try_collect::<Vec<_>>();
            // Like a local `remove_dir_all`: one failed book doesn't stop the rest.
            if self.block_on(removed).is_ok() {
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    async fn read_shelf_file(&self, name: &str) -> Result<Option<String>> {
        match self.get(&self.key(&format!(".shelf/{name}"))?).await? {
            Some(bytes) => Ok(Some(String::from_utf8(bytes.into())?)),
            None => Ok(None),
        }
    }

    async fn write_shelf_file(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let key = self.key(&format!(".shelf/{name}"))?;
        self.put(&key, Bytes::copy_from_slice(bytes)).await
    }

    /// Nothing to clean: a PUT never leaves half an object behind.
    async fn clean_up_temp_files(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use object_store::{ObjectStoreExt, memory::InMemory};
    use shelf_proto::{
        bookkeeping::{BookLocation, EpubProgress, book_file_type},
        config::Config,
        state::State,
    };

    use super::*;

    const ID: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
    const OTHER_ID: &str = "0000000000000000000000000000000000000000";

    async fn storage(keys: &[&str]) -> S3LibraryStorage {
        let store = Arc::new(InMemory::new());
        for key in keys {
            store.put(&Key::parse(key).unwrap(), "x".into()).await.unwrap();
        }
        S3LibraryStorage::with_store(store, "lib").unwrap()
    }

    /// Runs blocking storage calls off the runtime's thread, as callers must.
    async fn blocking<T: Send + 'static>(
        storage: &S3LibraryStorage,
        f: impl FnOnce(S3LibraryStorage) -> T + Send + 'static,
    ) -> T {
        let storage = storage.clone();
        tokio::task::spawn_blocking(move || f(storage)).await.unwrap()
    }

    fn sorted(set: HashSet<PathBuf>) -> Vec<PathBuf> {
        let mut paths: Vec<_> = set.into_iter().collect();
        paths.sort();
        paths
    }

    #[tokio::test]
    async fn lists_books_relative_to_the_prefix() {
        let storage = storage(&[
            "lib/top.epub",
            "lib/a/b.EPUB",
            "lib/a/notes.txt",
            "lib/a/.c.epub",
            "lib/.hidden/d.epub",
            "lib/.shelf/books/x.epub",
            "other/e.epub",
        ])
        .await;

        let (all, under, folders) = blocking(&storage, |s| {
            let folders = ["a", "top.epub", "nope"].map(|p| s.is_folder(Path::new(p)).unwrap());
            (
                s.all_book_file_paths().unwrap(),
                s.book_file_paths_under(Path::new("a")).unwrap(),
                folders,
            )
        })
        .await;

        assert_eq!(sorted(all), [PathBuf::from("a/b.EPUB"), "top.epub".into()]);
        assert_eq!(sorted(under), [PathBuf::from("a/b.EPUB")]);
        assert_eq!(folders, [true, false, false]);
    }

    #[tokio::test]
    async fn gone_book_is_none() {
        let storage = storage(&["lib/a.epub"]).await;

        let (modified, book, gone_modified, gone_book) = blocking(&storage, |s| {
            let (a, gone) = (Path::new("a.epub"), Path::new("gone.epub"));
            (
                s.book_modified(a).unwrap(),
                s.read_book(a).unwrap(),
                s.book_modified(gone).unwrap(),
                s.read_book(gone).unwrap(),
            )
        })
        .await;

        assert!(modified.is_some());
        assert_eq!(book.as_deref(), Some(&b"x"[..]));
        assert_eq!((gone_modified, gone_book), (None, None));
        assert!(storage.book_file(Path::new("gone.epub")).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn book_data_round_trips_and_cleans_up() {
        let storage = storage(&[]).await;
        let meta = BookMetadata {
            locations: vec![BookLocation {
                path: "a.epub".into(),
                ..Default::default()
            }],
            id: ID.into(),
            ..Default::default()
        };
        let progress = BookProgress {
            file_type: book_file_type::Value::Epub as i32,
            epub_progress: Some(EpubProgress { cfi: "epubcfi(/6/2)".into() }),
            user: "default".into(),
        };

        let (stored, stored_progress, missing, hashes, cleaned) = blocking(&storage, {
            let (meta, progress) = (meta.clone(), progress.clone());
            move |s| {
                s.set_book_metadata(ID, &meta).unwrap();
                s.set_book_progress(ID, "default", progress.clone()).unwrap();
                s.set_book_thumbnail(ID, b"jpeg").unwrap();
                s.set_book_metadata(OTHER_ID, &meta).unwrap();
                s.set_book_thumbnail(OTHER_ID, b"jpeg").unwrap();
                s.set_book_progress(OTHER_ID, "metadata", progress.clone()).unwrap();
                let hashes = s.all_book_hashes_in_books_dir().unwrap();
                let stored = (s.book_metadata(ID).unwrap(), s.book_progress(ID, "default").unwrap());
                let other_user = s.book_progress(ID, "other").unwrap();
                assert_eq!(other_user, None);
                let missing = (
                    s.book_metadata("nope").unwrap(),
                    s.book_progress("nope", "default").unwrap(),
                );
                let other = HashSet::from([OTHER_ID.to_owned()]);
                let cleaned = (s.clean_up_books(other.clone()).unwrap(), s.clean_up_books(other).unwrap());
                assert_eq!(s.book_metadata(OTHER_ID).unwrap(), None);
                // A user named "metadata" has a `metadata.json` too; only the book's goes.
                assert!(s.book_progress(OTHER_ID, "metadata").unwrap().is_some());
                let keep = HashSet::from([ID.to_owned()]);
                assert_eq!(s.clean_up_progress(&keep).unwrap(), 1);
                assert_eq!(s.all_book_hashes_in_books_dir().unwrap(), keep);
                (stored.0, stored.1, missing, hashes, cleaned)
            }
        })
        .await;

        assert_eq!(stored, Some(meta));
        assert_eq!(stored_progress, Some(progress));
        assert_eq!(missing, (None, None));
        assert_eq!(hashes, HashSet::from([ID.to_owned(), OTHER_ID.to_owned()]));
        assert_eq!(cleaned, (1, 0));
        assert!(matches!(
            storage.book_thumbnail(ID).await.unwrap(),
            Some(FileContent::Bytes(b)) if b == "jpeg"
        ));
        assert!(storage.book_thumbnail(OTHER_ID).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn state_and_config_round_trip() {
        let storage = storage(&[]).await;
        assert_eq!(storage.read_state("a").await.unwrap(), State::default());
        assert_eq!(storage.read_config("a").await.unwrap(), Config::default());

        let state = State {
            last_book_id: Some(ID.into()),
        };
        storage.write_state("a", &state).await.unwrap();
        let patch: Config =
            serde_json::from_value(serde_json::json!({"reader": {"flow": "PAGINATED"}})).unwrap();
        let config = storage.update_config("a", &patch).await.unwrap();

        assert_eq!(storage.read_state("a").await.unwrap(), state);
        assert_eq!(storage.read_state("b").await.unwrap(), State::default());
        assert_eq!(storage.read_config("a").await.unwrap(), config);
        assert_eq!(storage.read_config("b").await.unwrap(), Config::default());
        assert_eq!(config, patch);
    }
}
