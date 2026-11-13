//! A book's copies (`BookMetadata.locations`), kept sorted by path.

use shelf_proto::bookkeeping::{BookLocation, BookMetadata};

pub fn location<'a>(book: &'a BookMetadata, path: &str) -> Option<&'a BookLocation> {
    book.locations.iter().find(|l| l.path == path)
}

/// Adds or replaces the copy at `location.path`. Returns whether anything changed.
pub fn upsert_location(book: &mut BookMetadata, location: BookLocation) -> bool {
    match book
        .locations
        .binary_search_by(|l| l.path.as_str().cmp(&location.path))
    {
        Ok(i) if book.locations[i] == location => false,
        Ok(i) => {
            book.locations[i] = location;
            true
        }
        Err(i) => {
            book.locations.insert(i, location);
            true
        }
    }
}

/// Returns whether the book had a copy matching `gone`.
pub fn remove_locations(book: &mut BookMetadata, gone: impl Fn(&str) -> bool) -> bool {
    let len = book.locations.len();
    book.locations.retain(|l| !gone(&l.path));
    book.locations.len() != len
}

/// Drops `gone`'s copies from every book, and the books left without any. Returns whether the
/// index changed.
pub fn remove_from_index(index: &mut Vec<BookMetadata>, gone: impl Fn(&str) -> bool) -> bool {
    let mut changed = false;
    for book in index.iter_mut() {
        changed |= remove_locations(book, &gone);
    }
    index.retain(|b| !b.locations.is_empty());
    changed
}

/// Matches paths under `folder`, a library path. The trailing slash keeps `old/` from matching
/// `older/`.
pub fn under(folder: &str) -> impl Fn(&str) -> bool {
    let prefix = format!("{folder}/");
    move |path: &str| path.starts_with(&prefix)
}
