use std::path::PathBuf;

use crate::library::LibraryError;

/// One file or folder name from a client. Never hidden, so it can't reach `.shelf/` or be skipped
/// by the scanner.
pub fn entry_name(name: &str) -> Result<&str, LibraryError> {
    let name = name.trim();
    let invalid = |why: &str| Err(LibraryError::Invalid(format!("{name:?} {why}")));
    if name.is_empty() {
        return Err(LibraryError::Invalid("the name is empty".to_owned()));
    }
    if name.len() > 255 {
        return invalid("is longer than 255 bytes");
    }
    if name.contains(['/', '\\', '\0']) {
        return invalid("can't contain / or \\");
    }
    if name.starts_with('.') {
        return invalid("can't start with a dot");
    }
    Ok(name)
}

/// A folder path relative to the library root, from a client. `""` is the root.
pub fn folder_path(path: &str) -> Result<PathBuf, LibraryError> {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return Ok(PathBuf::new());
    }
    path.split('/')
        .map(|part| match entry_name(part) {
            Ok(name) if name == part => Ok(name),
            Ok(_) => Err(LibraryError::Invalid(format!("{path:?} isn't a folder path"))),
            Err(e) => Err(e),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_stay_in_their_folder() {
        assert_eq!(entry_name(" Dune.epub ").unwrap(), "Dune.epub");
        for bad in ["", "  ", ".", "..", ".shelf", ".hidden.epub", "a/b", "a\\b", "a\0b"] {
            assert!(entry_name(bad).is_err(), "{bad:?}");
        }
        assert!(entry_name(&"a".repeat(256)).is_err());
    }

    #[test]
    fn folder_paths_are_relative_and_visible() {
        assert_eq!(folder_path("").unwrap(), PathBuf::new());
        assert_eq!(folder_path("/sci-fi/old/").unwrap(), PathBuf::from("sci-fi/old"));
        for bad in ["a//b", "../x", "a/../b", "a/.shelf", ".shelf/books", "a/ b"] {
            assert!(folder_path(bad).is_err(), "{bad:?}");
        }
    }
}
