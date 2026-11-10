use std::{io::Cursor, path::PathBuf, sync::Arc};

use anyhow::Context;
use fast_image_resize::Resizer;
use image::{DynamicImage, codecs::jpeg::JpegEncoder};
use rbook::{
    Epub,
    ebook::toc::TocEntryKind,
    epub::manifest::{EpubManifest, EpubManifestEntry},
};
use sha1::{Digest, Sha1};
use shelf_proto::bookkeeping::{BookMetadata, book_file_type};
use tracing::{debug, info, warn};

use crate::storage::{LibraryStorage, LocalLibraryStorage};
use shelf_utility::timeutil::Time;

/// Only the book's path is known; `read` loads the book.
pub struct Uncached;

/// What `read` found on disk.
pub enum Cached {
    /// The book's bytes, their SHA-1, and the `metadata.json` stored under it, if any.
    Present {
        book: Arc<[u8]>,
        hash: String,
        metadata: Option<BookMetadata>,
    },
    /// The file is gone. `hash` is the id whose `metadata.json` still has its path, if any.
    Gone { hash: Option<String> },
}

/// Blocking throughout: use it inside `spawn_blocking`.
pub struct EbookExtractor<S = Uncached> {
    storage: LocalLibraryStorage,
    relative_path: PathBuf,
    state: S,
}

impl<S> EbookExtractor<S> {
    /// The book's path relative to the library root, as stored in `metadata.path`.
    pub fn rel_path(&self) -> String {
        self.relative_path.to_string_lossy().into_owned()
    }

    /// The file's modified time, at the precision `last_update_time` is stored with. `None` if
    /// the file is gone.
    fn modified(&self) -> anyhow::Result<Option<Time>> {
        Ok(self
            .storage
            .book_modified(&self.relative_path)?
            .map(Time::whole_seconds))
    }

    /// `known`'s id when it describes this file and the file hasn't been modified since
    /// `known` was written, so the book needn't be read and hashed again.
    pub fn predict_id_change_using_modified_time(
        &self,
        known: &BookMetadata,
    ) -> anyhow::Result<Option<String>> {
        if known.path != self.rel_path() || known.id.is_empty() {
            return Ok(None);
        }
        let Some(modified) = self.modified()? else {
            return Ok(None);
        };
        Ok(stamp_covers(known, modified).then(|| known.id.clone()))
    }

    fn stamp_covers_mtime(&self, known: &BookMetadata) -> anyhow::Result<bool> {
        let modified = self.modified()?.context("the book is gone")?;
        Ok(stamp_covers(known, modified))
    }
}

/// Whether `known` was written no earlier than the file's (whole-second) `modified` time.
fn stamp_covers(known: &BookMetadata, modified: Time) -> bool {
    // Missing in metadata from before the field existed; hashing once adds it.
    let Some(updated) = known
        .last_update_time
        .as_deref()
        .and_then(|t| t.parse::<Time>().ok())
    else {
        return false;
    };
    modified <= updated
}

impl EbookExtractor<Uncached> {
    /// `relative_path` is relative to the library root.
    pub fn new(storage: LocalLibraryStorage, relative_path: PathBuf) -> Self {
        Self {
            storage,
            relative_path,
            state: Uncached,
        }
    }

    /// Reads and hashes the book, and loads its `metadata.json` if it has one. For a gone file,
    /// scans every `metadata.json` for the id that still has its path.
    pub fn read(self) -> anyhow::Result<EbookExtractor<Cached>> {
        let state = if let Some(book) = self.storage.read_book(&self.relative_path)? {
            let book: Arc<[u8]> = book.into();
            let hash = sha1_hex(&book);
            // Failing here would leave the id unproduced, and the startup sync would delete the
            // book's folder, progress included. Missing means it's extracted again instead.
            let metadata = self.storage.book_metadata(&hash).unwrap_or_else(|e| {
                warn!(id = %hash, error = %format!("{e:#}"), "unreadable metadata, extracting again");
                None
            });
            Cached::Present {
                book,
                hash,
                metadata,
            }
        } else {
            let rel_path = self.rel_path();
            let hash = self
                .storage
                .all_book_metadata()?
                .into_iter()
                .find(|m| m.path == rel_path)
                .map(|m| m.id);
            Cached::Gone { hash }
        };
        Ok(EbookExtractor {
            storage: self.storage,
            relative_path: self.relative_path,
            state,
        })
    }
}

impl EbookExtractor<Cached> {
    // The largest a thumbnail gets; it keeps the cover's own shape inside this box.
    const THUMBNAIL_WIDTH: u32 = 400;
    const THUMBNAIL_HEIGHT: u32 = 600;
    const THUMBNAIL_JPEG_QUALITY: u8 = 85;

    /// The SHA-1 of the book's bytes, or `None` if the file is gone.
    pub fn hash(&self) -> Option<&str> {
        match &self.state {
            Cached::Present { hash, .. } => Some(hash),
            Cached::Gone { .. } => None,
        }
    }

    /// Brings `.shelf/books/<hash>/` up to date and returns the book's id and metadata. Without
    /// `force`, known metadata only gets its `path` and `last_update_time` refreshed. A gone book
    /// has no metadata, and an id only if `.shelf/` still had one for its path.
    pub fn extract(&self, force: bool) -> anyhow::Result<(Option<String>, Option<BookMetadata>)> {
        debug!(path = %self.rel_path(), force, "extracting");
        let (book, hash, metadata) = match &self.state {
            Cached::Gone { hash } => return Ok((hash.clone(), None)),
            Cached::Present {
                book,
                hash,
                metadata,
            } => (book, hash, metadata),
        };
        let meta = match metadata {
            Some(known) if !force => self.refresh(hash, known.clone())?,
            _ => self.extract_full(book, hash)?,
        };
        Ok((Some(hash.clone()), Some(meta)))
    }

    fn refresh(&self, hash: &str, mut meta: BookMetadata) -> anyhow::Result<BookMetadata> {
        debug!(path = %self.rel_path(), id = hash, "refreshing metadata");
        let mut changed = false;
        if meta.path != self.rel_path() {
            meta.path = self.rel_path();
            changed = true;
        }
        if meta.id != hash {
            meta.id = hash.to_owned();
            changed = true;
        }
        // Otherwise a touched but unchanged file would be hashed on every start.
        if !self.stamp_covers_mtime(&meta)? {
            meta.last_update_time = Some(Time::now().to_string());
            changed = true;
        }
        if changed {
            self.storage.set_book_metadata(hash, &meta)?;
        }
        Ok(meta)
    }

    fn extract_full(&self, book: &Arc<[u8]>, hash: &str) -> anyhow::Result<BookMetadata> {
        let path = self.rel_path();
        debug!(path = %path, id = hash, bytes = book.len(), "full extraction started");
        let book = Epub::read(Cursor::new(Arc::clone(book))).context("parsing the EPUB")?;
        self.storage.create_book_dir(hash)?;
        let metadata = book.metadata();
        let meta = BookMetadata {
            authors: metadata.creators().map(|a| a.to_string()).collect(),
            title: metadata.title().map(|t| t.to_string()),
            path: path.clone(),
            file_type: book_file_type::Value::Epub as i32,
            id: hash.to_owned(),
            last_update_time: Some(Time::now().to_string()),
        };
        self.storage.set_book_metadata(hash, &meta)?;

        // TODO: write a zero progress file for the book

        let cover = find_cover(&book);
        let thumbnail = cover.as_ref().is_some_and(|(cover, found_by)| {
            match self.write_thumbnail(hash, cover) {
                Ok((cover_bytes, thumbnail_bytes)) => {
                    debug!(
                        path = %path,
                        cover = %cover.href(),
                        found_by,
                        media_type = cover.media_type(),
                        cover_bytes,
                        thumbnail_bytes,
                        "thumbnail written"
                    );
                    true
                }
                // A bad cover shouldn't cost the book its listing (or a known book its progress).
                Err(e) => {
                    warn!(
                        path = %path,
                        cover = %cover.href(),
                        media_type = cover.media_type(),
                        error = %format!("{e:#}"),
                        "couldn't make a thumbnail, keeping the book without one"
                    );
                    false
                }
            }
        });

        let missing: Vec<&str> = [
            ("title", meta.title.is_none()),
            ("authors", meta.authors.is_empty()),
            ("cover", cover.is_none()),
            ("thumbnail", cover.is_some() && !thumbnail),
        ]
        .into_iter()
        .filter_map(|(field, is_missing)| is_missing.then_some(field))
        .collect();
        info!(
            path = %path,
            id = hash,
            title = ?meta.title,
            authors = ?meta.authors,
            cover = cover.is_some(),
            thumbnail,
            ?missing,
            "book extracted"
        );
        Ok(meta)
    }

    /// Returns the cover's size and the thumbnail's, in bytes.
    fn write_thumbnail(
        &self,
        hash: &str,
        cover: &EpubManifestEntry,
    ) -> anyhow::Result<(usize, usize)> {
        let image = cover.read_bytes().context("reading the cover")?;
        let thumbnail = self
            .resize_image_to_thumbnail(&image)
            .context("making the thumbnail")?;
        self.storage.set_book_thumbnail(hash, &thumbnail)?;
        Ok((image.len(), thumbnail.len()))
    }

    /// Scales the whole cover down to fit the thumbnail box, never cropping or enlarging it.
    fn resize_image_to_thumbnail(&self, image: &[u8]) -> anyhow::Result<Vec<u8>> {
        let src = DynamicImage::ImageRgb8(image::load_from_memory(image)?.into_rgb8());
        let (width, height) = (src.width() as f64, src.height() as f64);
        let scale = (Self::THUMBNAIL_WIDTH as f64 / width)
            .min(Self::THUMBNAIL_HEIGHT as f64 / height)
            .min(1.0);
        let size = |side: f64| ((side * scale).round() as u32).max(1);
        let mut dst = DynamicImage::new_rgb8(size(width), size(height));
        Resizer::new().resize(&src, &mut dst, None)?;

        let mut jpeg = Vec::new();
        dst.write_with_encoder(JpegEncoder::new_with_quality(
            &mut jpeg,
            Self::THUMBNAIL_JPEG_QUALITY,
        ))?;
        Ok(jpeg)
    }
}

/// The book's cover image and how it was found. Falls back for EPUBs that declare it wrongly:
/// first the declared cover, then the image on the item `cover`, then the EPUB 2 cover `<meta>`
/// read as a path instead of an id, then the image on the landmarks/guide cover page.
fn find_cover<'a>(epub: &'a Epub) -> Option<(EpubManifestEntry<'a>, &'static str)> {
    let manifest = epub.manifest();
    if let Some(cover) = manifest.cover_image() {
        return Some((cover, "declared"));
    }
    // Before the meta and the guide: some books point both at a placeholder put in front.
    if let Some(cover) = manifest
        .by_id("cover")
        .and_then(|page| image_on_page(&manifest, page))
    {
        return Some((cover, "cover item"));
    }
    if let Some(cover) = epub
        .metadata()
        .by_property("cover")
        .find_map(|meta| manifest.by_href(meta.value()))
        .filter(|entry| entry.kind().is_image())
    {
        return Some((cover, "meta path"));
    }
    let landmarks = epub.toc().landmarks();
    landmarks
        .iter()
        .flat_map(|root| root.flatten())
        .find(|entry| entry.kind() == TocEntryKind::Cover)
        .and_then(|entry| entry.manifest_entry())
        .and_then(|page| image_on_page(&manifest, page))
        .map(|cover| (cover, "landmark page"))
}

/// `page` itself when it's an image, else the first image the XHTML page shows.
fn image_on_page<'a>(
    manifest: &EpubManifest<'a>,
    page: EpubManifestEntry<'a>,
) -> Option<EpubManifestEntry<'a>> {
    if page.kind().is_image() {
        return Some(page);
    }
    let xhtml = page.read_str().ok()?;
    let src = first_image_src(&xhtml)?;
    manifest
        .by_href(&resolve_href(page.href().as_str(), src))
        .filter(|entry| entry.kind().is_image())
}

/// The target of the first `<img src>` or SVG `<image href>` / `xlink:href`.
fn first_image_src(xhtml: &str) -> Option<&str> {
    let start = ["<img", "<image"]
        .iter()
        .filter_map(|tag| xhtml.find(tag))
        .min()?;
    let tag = &xhtml[start..];
    let tag = &tag[..tag.find('>')?];
    ["src=", "href="].iter().find_map(|attr| {
        let value = &tag[tag.find(attr)? + attr.len()..];
        let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
        let value = &value[1..];
        Some(&value[..value.find(quote)?])
    })
}

/// Resolves `relative` against the absolute in-book path of the page it appears on.
fn resolve_href(page: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            // Never above the book's root (the leading empty part).
            ".." if parts.len() > 1 => {
                parts.pop();
            }
            ".." => {}
            part => parts.push(part),
        }
    }
    parts.join("/")
}

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        time::UNIX_EPOCH,
    };

    use super::*;
    use shelf_utility::timeutil::Duration;

    /// 2025-10-26T04:45:00Z
    const MTIME: u64 = 1_761_453_900;
    /// The SHA-1 of an empty file.
    const EMPTY_SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

    /// `predict_id_change_using_modified_time` and `refresh` only look at the file's mtime, so an empty file stands in for a book.
    fn book(nanos: u32) -> (tempfile::TempDir, EbookExtractor) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("dune.epub");
        File::create(&path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::new(MTIME, nanos))
            .unwrap();
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        let extractor = EbookExtractor::new(storage, "dune.epub".into());
        (root, extractor)
    }

    fn known(path: &str, updated: Option<&str>) -> BookMetadata {
        BookMetadata {
            path: path.into(),
            id: "abc".into(),
            last_update_time: updated.map(Into::into),
            ..Default::default()
        }
    }

    #[test]
    fn unchanged_when_not_modified_since() {
        let (_root, book) = book(500_000_000);
        let id = Some("abc".to_owned());
        // Written in the same second as the file: the stamp has no fraction to compare.
        assert_eq!(
            book.predict_id_change_using_modified_time(&known(
                "dune.epub",
                Some("2025-10-26T04:45:00Z")
            ))
            .unwrap(),
            id
        );
        assert_eq!(
            book.predict_id_change_using_modified_time(&known(
                "dune.epub",
                Some("2025-10-26T05:00:00Z")
            ))
            .unwrap(),
            id
        );
    }

    #[test]
    fn changed_when_modified_since() {
        let (_root, book) = book(0);
        assert_eq!(
            book.predict_id_change_using_modified_time(&known(
                "dune.epub",
                Some("2025-10-26T04:44:59Z")
            ))
            .unwrap(),
            None
        );
    }

    #[test]
    fn changed_when_path_or_stamp_differs() {
        let (_root, book) = book(0);
        let later = Some("2025-10-26T05:00:00Z");
        assert_eq!(
            book.predict_id_change_using_modified_time(&known("other.epub", later))
                .unwrap(),
            None
        );
        assert_eq!(
            book.predict_id_change_using_modified_time(&known("dune.epub", None))
                .unwrap(),
            None
        );
        assert_eq!(
            book.predict_id_change_using_modified_time(&known("dune.epub", Some("garbage")))
                .unwrap(),
            None
        );
    }

    #[test]
    fn extract_without_force_refreshes_path_and_stamp_only() {
        let (root, book) = book(0);
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        fs::create_dir_all(storage.book_dir_by_id(EMPTY_SHA1)).unwrap();
        let old = BookMetadata {
            title: Some("Dune".into()),
            authors: vec!["Frank Herbert".into()],
            id: EMPTY_SHA1.into(),
            ..known("old/dune.epub", Some("2025-10-26T04:44:59Z"))
        };
        storage.set_book_metadata(EMPTY_SHA1, &old).unwrap();

        let book = book.read().unwrap();
        let (hash, meta) = book.extract(false).unwrap();
        let meta = meta.unwrap();

        assert_eq!(hash.as_deref(), Some(EMPTY_SHA1));
        assert_eq!(meta.path, "dune.epub");
        assert_eq!((&meta.title, &meta.authors), (&old.title, &old.authors));
        // The next startup can skip it.
        assert_eq!(
            book.predict_id_change_using_modified_time(&meta)
                .unwrap()
                .as_deref(),
            Some(EMPTY_SHA1)
        );
        assert_eq!(storage.book_metadata(EMPTY_SHA1).unwrap(), Some(meta));
    }

    /// Deletes the book `book()` made and returns an extractor for its path, made afterwards.
    fn gone(root: &tempfile::TempDir) -> EbookExtractor {
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        fs::remove_file(storage.library_root().join("dune.epub")).unwrap();
        EbookExtractor::new(storage, "dune.epub".into())
    }

    #[test]
    fn gone_book_returns_its_id_from_shelf() {
        let (root, _) = book(0);
        let storage = LocalLibraryStorage::new(root.path()).unwrap();
        fs::create_dir_all(storage.book_dir_by_id(EMPTY_SHA1)).unwrap();
        let meta = BookMetadata {
            id: EMPTY_SHA1.into(),
            ..known("dune.epub", None)
        };
        storage.set_book_metadata(EMPTY_SHA1, &meta).unwrap();

        let book = gone(&root);
        assert_eq!(
            book.predict_id_change_using_modified_time(&meta).unwrap(),
            None
        );
        let (hash, meta) = book.read().unwrap().extract(false).unwrap();
        assert_eq!((hash.as_deref(), meta), (Some(EMPTY_SHA1), None));
    }

    #[test]
    fn finds_the_first_image_on_a_cover_page() {
        let img = r#"<body><div><img alt="Image" src="../images/cover.jpg"/></div></body>"#;
        assert_eq!(first_image_src(img), Some("../images/cover.jpg"));
        let svg = r#"<svg><image width="600" xlink:href='cover.png'/></svg><img src="b.jpg"/>"#;
        assert_eq!(first_image_src(svg), Some("cover.png"));
        assert_eq!(first_image_src("<p>no image</p>"), None);
    }

    #[test]
    fn resolves_hrefs_against_the_page() {
        let page = "/OEBPS/Text/cover.xhtml";
        assert_eq!(resolve_href(page, "../Images/a.jpg"), "/OEBPS/Images/a.jpg");
        assert_eq!(resolve_href(page, "./a.jpg"), "/OEBPS/Text/a.jpg");
        assert_eq!(resolve_href(page, "../../../a.jpg"), "/a.jpg");
    }

    #[test]
    fn gone_book_without_metadata_has_no_id() {
        let (root, _) = book(0);
        let (hash, meta) = gone(&root).read().unwrap().extract(false).unwrap();
        assert_eq!((hash, meta), (None, None));
    }
}
