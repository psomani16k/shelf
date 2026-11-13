use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use shelf_proto::bookkeeping::BookMetadata;

use crate::library::names::folder_path;

/// The whole library, as a local library and an unscoped user see it.
pub static EVERYTHING: Scope = Scope {
    root: String::new(),
    books: None,
};

/// The part of the library one user sees: a root folder, which their paths are relative to, and
/// the books under it that match their patterns. Everything else acts as if it didn't exist.
/// The default is the whole library.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// Relative to the library root, `/`-separated, no trailing slash; `""` is the whole library.
    root: String,
    books: Option<Patterns>,
}

#[derive(Debug, Clone)]
struct Patterns {
    set: GlobSet,
    negated: Vec<bool>,
    /// For a path no pattern matches.
    default: bool,
}

impl Scope {
    /// `root` is a folder path relative to the library. `patterns` are globs relative to it,
    /// matched against book paths ignoring case; a leading `!` excludes and the last match wins.
    /// With no positive pattern every book starts visible, otherwise none does.
    pub fn new(root: &str, patterns: &[String]) -> Result<Self> {
        let root = folder_path(root)
            .map_err(|e| anyhow!("bad library root {root:?}: {e:?}"))?
            .to_string_lossy()
            .into_owned();
        let books = if patterns.is_empty() {
            None
        } else {
            Some(Patterns::new(patterns)?)
        };
        Ok(Self { root, books })
    }

    /// The root as stored: `/`-separated, no leading or trailing slash; `""` is the whole library.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Whether some books under the root may be hidden.
    pub fn has_patterns(&self) -> bool {
        self.books.is_some()
    }

    /// A path from the client, relative to the user's root, as a path in the library.
    pub fn to_library(&self, client: &Path) -> PathBuf {
        // `join("")` would add a trailing slash.
        if client.as_os_str().is_empty() {
            return PathBuf::from(&self.root);
        }
        Path::new(&self.root).join(client)
    }

    /// A library path relative to the user's root; `None` if it's outside it.
    pub fn to_client<'a>(&self, library: &'a str) -> Option<&'a str> {
        if self.root.is_empty() {
            return Some(library);
        }
        let rest = library.strip_prefix(&self.root)?;
        match rest.strip_prefix('/') {
            Some(rest) => Some(rest),
            None => rest.is_empty().then_some(rest),
        }
    }

    /// Whether the book at this library path is visible to the user.
    pub fn allows(&self, library: &str) -> bool {
        self.to_client(library)
            .is_some_and(|rel| self.books.as_ref().is_none_or(|p| p.allows(rel)))
    }

    /// Whether any copy of the book is visible.
    pub fn allows_book(&self, book: &BookMetadata) -> bool {
        book.locations.iter().any(|l| self.allows(&l.path))
    }

    /// A folder (a library path) is hidden when the index has books under it and none of them
    /// is visible. The user's root, and folders with no books, always show.
    pub fn shows_folder(&self, index: &[BookMetadata], library: &str) -> bool {
        if !self.has_patterns() || library == self.root {
            return true;
        }
        if self.to_client(library).is_none() {
            return false;
        }
        let prefix = format!("{library}/");
        let mut has_books = false;
        for path in index.iter().flat_map(|b| &b.locations).map(|l| &l.path) {
            if path.starts_with(&prefix) {
                if self.allows(path) {
                    return true;
                }
                has_books = true;
            }
        }
        !has_books
    }
}

impl Patterns {
    fn new(patterns: &[String]) -> Result<Self> {
        let mut set = GlobSetBuilder::new();
        let mut negated = Vec::new();
        for pattern in patterns {
            let (negate, glob) = match pattern.strip_prefix('!') {
                Some(glob) => (true, glob),
                None => (false, pattern.as_str()),
            };
            if glob.is_empty() {
                bail!("empty book pattern {pattern:?}");
            }
            let glob = GlobBuilder::new(glob)
                .literal_separator(true)
                .case_insensitive(true)
                .build()
                .with_context(|| format!("bad book pattern {pattern:?}"))?;
            set.add(glob);
            negated.push(negate);
        }
        let default = negated.iter().all(|&n| n);
        Ok(Self {
            set: set.build()?,
            negated,
            default,
        })
    }

    fn allows(&self, rel: &str) -> bool {
        match self.set.matches(rel).into_iter().max() {
            Some(last) => !self.negated[last],
            None => self.default,
        }
    }
}

#[cfg(test)]
mod tests {
    use shelf_proto::bookkeeping::BookLocation;

    use super::*;

    fn scope(root: &str, patterns: &[&str]) -> Scope {
        let patterns: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
        Scope::new(root, &patterns).unwrap()
    }

    fn book(path: &str) -> BookMetadata {
        BookMetadata {
            locations: vec![BookLocation {
                path: path.into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn the_root_is_joined_and_stripped() {
        let kid = scope("/kid/", &[]);

        assert_eq!(kid.to_library(Path::new("a/b.epub")), Path::new("kid/a/b.epub"));
        assert_eq!(kid.to_library(Path::new("")), Path::new("kid"));
        assert_eq!(kid.to_client("kid/a/b.epub"), Some("a/b.epub"));
        assert_eq!(kid.to_client("kid"), Some(""));
        assert_eq!(kid.to_client("kiddo/b.epub"), None);
        assert!(kid.allows("kid/b.epub"));
        assert!(!kid.allows("other/b.epub"));
        assert!(EVERYTHING.allows("anything/b.epub"));
    }

    #[test]
    fn the_last_matching_pattern_wins() {
        let s = scope("kid", &["**/*.epub", "!scary/**", "scary/ok.epub"]);

        assert!(s.allows("kid/dune.epub"));
        assert!(s.allows("kid/a/b/dune.EPUB"));
        assert!(!s.allows("kid/scary/it.epub"));
        assert!(s.allows("kid/scary/ok.epub"));
        assert!(!s.allows("dune.epub"));
    }

    #[test]
    fn a_star_stays_in_its_folder_and_no_match_hides() {
        let s = scope("", &["*.epub"]);

        assert!(s.allows("dune.epub"));
        assert!(!s.allows("a/dune.epub"));
    }

    #[test]
    fn only_exclusions_start_from_everything() {
        let s = scope("", &["!scary/**"]);

        assert!(s.allows("dune.epub"));
        assert!(!s.allows("scary/it.epub"));
    }

    #[test]
    fn bad_roots_and_patterns_are_rejected() {
        for root in ["../x", ".shelf", "a/../b"] {
            assert!(Scope::new(root, &[]).is_err(), "{root}");
        }
        for pattern in ["!", "a[", ""] {
            assert!(Scope::new("", &[pattern.to_owned()]).is_err(), "{pattern}");
        }
    }

    #[test]
    fn a_folder_with_only_hidden_books_is_hidden() {
        let s = scope("", &["!scary/**"]);
        let index = [book("scary/it.epub"), book("mixed/it.epub"), book("mixed/scary/x.epub")];

        assert!(!s.shows_folder(&index, "scary"));
        assert!(s.shows_folder(&index, "mixed"));
        assert!(s.shows_folder(&index, "empty"));
        assert!(s.shows_folder(&index, ""));
    }
}
