use anyhow::{Result, ensure};

/// A local library's user, until users can be picked there.
pub const DEFAULT_USER: &str = "default";

/// A username becomes a file name in `.shelf/`: 1 to 32 of `a–z`, `0–9`, `-`, `_`.
pub fn validate_user(name: &str) -> Result<()> {
    let allowed = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_';
    ensure!(
        (1..=32).contains(&name.len()) && name.bytes().all(allowed),
        "invalid user name {name:?}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_names_are_safe_file_names() {
        for ok in [DEFAULT_USER, "a", "parth_2", "x-y", &"a".repeat(32)] {
            assert!(validate_user(ok).is_ok(), "{ok}");
        }
        for bad in ["", "Parth", "../x", "a/b", "a.b", "a b", "é", &"a".repeat(33)] {
            assert!(validate_user(bad).is_err(), "{bad}");
        }
    }
}
