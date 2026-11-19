pub mod builtin;

use std::collections::HashMap;

use shelf_proto::{
    api::{ListThemesResponse, ThemeEntry, theme_source::Value as Source},
    config::{ReaderThemeSpec, ThemeLayer, theme_layer::Kind},
};
use tracing::warn;

use crate::{library::LibraryError, storage::LibraryStorage};
use builtin::BUILT_IN;

/// Per user, counting overrides of built-ins.
pub const MAX_THEMES: usize = 128;
/// Each layer is a matrix multiply when the theme is composed, but more than this is a mistake.
pub const MAX_LAYERS: usize = 32;
/// Used when the config names no theme, or one that's gone.
pub const DEFAULT_THEME: &str = "light";
const MAX_NAME: usize = 32;

/// A theme's id, from its name: lowercase, spaces as `-`, so it's also a safe file name.
pub fn theme_id(name: &str) -> Result<String, LibraryError> {
    let name = name.trim();
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_');
    if name.is_empty() || name.len() > MAX_NAME || !name.chars().all(allowed) {
        return Err(LibraryError::Invalid(format!(
            "a theme name is 1 to {MAX_NAME} of A–Z, a–z, 0–9, space, - and _, which {name:?} isn't"
        )));
    }
    Ok(id_of(name))
}

fn id_of(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace(' ', "-")
}

/// The theme's id, if the name and every layer are valid.
pub fn validate(spec: &ReaderThemeSpec) -> Result<String, LibraryError> {
    let id = theme_id(&spec.name)?;
    if spec.layers.len() > MAX_LAYERS {
        return Err(LibraryError::Invalid(format!(
            "a theme has at most {MAX_LAYERS} layers"
        )));
    }
    for (i, layer) in spec.layers.iter().enumerate() {
        if let Err(why) = check_layer(layer) {
            return Err(LibraryError::Invalid(format!("layer {} {why}", i + 1)));
        }
    }
    Ok(id)
}

fn check_layer(layer: &ThemeLayer) -> Result<(), &'static str> {
    let factor = |v: Option<f64>| v.is_none_or(|v| v.is_finite() && v >= 0.0);
    let (ok, why) = match &layer.kind {
        None => (false, "has no kind"),
        Some(Kind::Invert(_)) => (true, ""),
        Some(Kind::HueRotate(h)) => (h.degrees.is_finite(), "needs a finite angle"),
        Some(Kind::Saturate(s)) => (factor(s.amount), "needs a factor of 0 or more"),
        Some(Kind::BrightnessContrast(bc)) => (
            factor(bc.brightness) && factor(bc.contrast),
            "needs factors of 0 or more",
        ),
        Some(Kind::Levels(l)) => (
            [&l.black, &l.white].iter().all(|c| c.is_empty() || is_color(c)),
            "needs colours as #rrggbb",
        ),
        Some(Kind::Multiply(m)) => (is_color(&m.color), "needs a colour as #rrggbb"),
        Some(Kind::Matrix(m)) => (
            m.values.len() == 12 && m.values.iter().all(|v| v.is_finite()),
            "needs 12 finite numbers",
        ),
    };
    if ok { Ok(()) } else { Err(why) }
}

fn is_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// One of the user's themes, from the files that hold its id.
struct OwnTheme {
    id: String,
    spec: ReaderThemeSpec,
    /// The first is where `spec` came from; any others are duplicates the list ignores.
    files: Vec<String>,
}

/// Every file is read for its name, never trusting the file's own. The first file (by name) with
/// an id wins.
async fn own_themes(
    storage: &(impl LibraryStorage + ?Sized),
    user: &str,
) -> Result<Vec<OwnTheme>, LibraryError> {
    let mut themes: Vec<OwnTheme> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    for (file, mut spec) in storage.read_themes(user).await? {
        let Ok(id) = theme_id(&spec.name) else {
            warn!(%file, "skipped a theme with an invalid name");
            continue;
        };
        if let Some(&i) = by_id.get(&id) {
            warn!(%file, %id, "skipped a theme whose id another file has");
            themes[i].files.push(file);
            continue;
        }
        spec.name = spec.name.trim().to_owned();
        by_id.insert(id.clone(), themes.len());
        themes.push(OwnTheme {
            id,
            spec,
            files: vec![file],
        });
    }
    Ok(themes)
}

fn is_built_in(id: &str) -> bool {
    BUILT_IN.iter().any(|t| id_of(t.name) == id)
}

/// The built-ins in their order, each replaced by the user's theme with its id, then the user's
/// other themes by id.
pub async fn list_themes(
    storage: &(impl LibraryStorage + ?Sized),
    user: &str,
) -> Result<ListThemesResponse, LibraryError> {
    Ok(merge(own_themes(storage, user).await?))
}

fn merge(mut own: Vec<OwnTheme>) -> ListThemesResponse {
    let entry = |id: String, source: Source, spec: ReaderThemeSpec| ThemeEntry {
        id,
        source: source as i32,
        spec: Some(spec),
    };
    let mut themes = Vec::new();
    for built_in in BUILT_IN {
        let id = id_of(built_in.name);
        themes.push(match own.iter().position(|t| t.id == id) {
            Some(i) => entry(id, Source::Overridden, own.remove(i).spec),
            None => entry(id, Source::BuiltIn, built_in.to_spec()),
        });
    }
    own.sort_by(|a, b| a.id.cmp(&b.id));
    themes.extend(own.into_iter().map(|t| entry(t.id, Source::Custom, t.spec)));
    ListThemesResponse {
        themes,
        default_id: DEFAULT_THEME.to_owned(),
    }
}

/// Creates the theme, or replaces the user's one with its id. Callers hold the config lock.
pub async fn set_theme(
    storage: &(impl LibraryStorage + ?Sized),
    user: &str,
    mut spec: ReaderThemeSpec,
) -> Result<(), LibraryError> {
    spec.name = spec.name.trim().to_owned();
    let id = validate(&spec)?;
    let own = own_themes(storage, user).await?;
    if own.len() >= MAX_THEMES && !own.iter().any(|t| t.id == id) {
        return Err(LibraryError::Invalid(format!(
            "you can have at most {MAX_THEMES} themes"
        )));
    }
    write(storage, user, &own, &id, &spec).await
}

/// Writes `<id>.json` and deletes every other file holding `id`.
async fn write(
    storage: &(impl LibraryStorage + ?Sized),
    user: &str,
    own: &[OwnTheme],
    id: &str,
    spec: &ReaderThemeSpec,
) -> Result<(), LibraryError> {
    let file = format!("{id}.json");
    storage.write_theme(user, &file, spec).await?;
    for theme in own.iter().filter(|t| t.id == id) {
        for other in theme.files.iter().filter(|f| **f != file) {
            storage.delete_theme(user, other).await?;
        }
    }
    Ok(())
}

/// Callers hold the config lock.
pub async fn delete_theme(
    storage: &(impl LibraryStorage + ?Sized),
    user: &str,
    name: &str,
) -> Result<(), LibraryError> {
    let id = theme_id(name)?;
    let own = own_themes(storage, user).await?;
    let Some(theme) = own.iter().find(|t| t.id == id) else {
        return Err(not_own(&id, "deleted"));
    };
    for file in &theme.files {
        storage.delete_theme(user, file).await?;
    }
    Ok(())
}

/// Returns the old and new ids. Callers hold the config lock and move the config's choice.
pub async fn rename_theme(
    storage: &(impl LibraryStorage + ?Sized),
    user: &str,
    from: &str,
    to: &str,
) -> Result<(String, String), LibraryError> {
    let from_id = theme_id(from)?;
    let to_id = theme_id(to)?;
    let own = own_themes(storage, user).await?;
    let Some(old) = own.iter().find(|t| t.id == from_id) else {
        return Err(not_own(&from_id, "renamed"));
    };
    if to_id != from_id && own.iter().any(|t| t.id == to_id) {
        return Err(LibraryError::Conflict(format!(
            "there's already a theme called {:?}",
            to.trim()
        )));
    }
    let spec = ReaderThemeSpec {
        name: to.trim().to_owned(),
        ..old.spec.clone()
    };
    write(storage, user, &own, &to_id, &spec).await?;
    if to_id != from_id {
        for file in &old.files {
            storage.delete_theme(user, file).await?;
        }
    }
    Ok((from_id, to_id))
}

fn not_own(id: &str, what: &str) -> LibraryError {
    if is_built_in(id) {
        LibraryError::Invalid(format!("built-in themes can't be {what}"))
    } else {
        LibraryError::NotFound
    }
}

/// The user's own themes, for an export.
pub async fn export_themes(
    storage: &(impl LibraryStorage + ?Sized),
    user: &str,
) -> Result<Vec<ReaderThemeSpec>, LibraryError> {
    Ok(own_themes(storage, user)
        .await?
        .into_iter()
        .map(|t| t.spec)
        .collect())
}

/// Writes each theme like `set_theme`, skipping invalid ones; returns how many were written.
/// Callers hold the config lock.
pub async fn import_themes(
    storage: &(impl LibraryStorage + ?Sized),
    user: &str,
    specs: Vec<ReaderThemeSpec>,
) -> Result<u32, LibraryError> {
    let mut written = 0;
    for spec in specs {
        match set_theme(storage, user, spec).await {
            Ok(()) => written += 1,
            Err(LibraryError::Invalid(why)) => warn!(%why, "skipped an imported theme"),
            Err(e) => return Err(e),
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use shelf_proto::config::theme_layer;

    use super::*;
    use crate::storage::LocalLibraryStorage;

    fn spec(name: &str) -> ReaderThemeSpec {
        ReaderThemeSpec {
            name: name.to_owned(),
            layers: vec![ThemeLayer {
                skip_images: true,
                kind: Some(Kind::Invert(theme_layer::Invert {})),
            }],
        }
    }

    fn storage(root: &tempfile::TempDir) -> LocalLibraryStorage {
        LocalLibraryStorage::new(root.path()).unwrap()
    }

    async fn listed(s: &LocalLibraryStorage) -> Vec<(String, i32, String)> {
        let list = list_themes(s, "a").await.unwrap();
        list.themes
            .into_iter()
            .map(|t| (t.id, t.source, t.spec.unwrap().name))
            .collect()
    }

    #[test]
    fn ids_come_from_names() {
        assert_eq!(theme_id(" My Night ").unwrap(), "my-night");
        assert_eq!(theme_id("AMOLED").unwrap(), "amoled");
        for bad in ["", "  ", "a/b", "../x", ".hidden", "é", "a.b", &"a".repeat(33)] {
            assert!(theme_id(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn built_ins_are_valid_and_distinct() {
        let mut ids = HashSet::new();
        for theme in BUILT_IN {
            let id = validate(&theme.to_spec()).unwrap();
            assert!(ids.insert(id), "{}", theme.name);
        }
        assert!(ids.contains(DEFAULT_THEME));
    }

    #[test]
    fn bad_layers_are_invalid() {
        let layer = |kind| ReaderThemeSpec {
            name: "x".to_owned(),
            layers: vec![ThemeLayer {
                skip_images: false,
                kind,
            }],
        };
        let bad = [
            None,
            Some(Kind::HueRotate(theme_layer::HueRotate { degrees: f64::NAN })),
            Some(Kind::Saturate(theme_layer::Saturate { amount: Some(-1.0) })),
            Some(Kind::Levels(theme_layer::Levels {
                black: "black".to_owned(),
                white: String::new(),
            })),
            Some(Kind::Multiply(theme_layer::Multiply { color: String::new() })),
            Some(Kind::Matrix(theme_layer::ColorMatrix { values: vec![1.0; 11] })),
        ];
        for kind in bad {
            assert!(validate(&layer(kind.clone())).is_err(), "{kind:?}");
        }
        let too_many = ReaderThemeSpec {
            layers: vec![spec("x").layers[0].clone(); MAX_LAYERS + 1],
            ..spec("x")
        };
        assert!(validate(&too_many).is_err());
    }

    #[tokio::test]
    async fn own_themes_override_built_ins_and_follow_them() {
        let root = tempfile::tempdir().unwrap();
        let s = storage(&root);
        set_theme(&s, "a", spec("zebra")).await.unwrap();
        set_theme(&s, "a", spec("DARK")).await.unwrap();
        set_theme(&s, "a", spec("Apple")).await.unwrap();

        let list = listed(&s).await;
        let ids: Vec<String> = list.iter().map(|(id, ..)| id.clone()).collect();
        let built_in: Vec<String> = BUILT_IN.iter().map(|t| id_of(t.name)).collect();
        let n = built_in.len();
        assert_eq!(ids[..n], built_in);
        assert_eq!(ids[n..], ["apple", "zebra"]);
        let dark = ids.iter().position(|id| id == "dark").unwrap();
        let light = ids.iter().position(|id| id == "light").unwrap();
        assert_eq!(list[dark], ("dark".to_owned(), Source::Overridden as i32, "DARK".to_owned()));
        assert_eq!(list[light].1, Source::BuiltIn as i32);
        assert_eq!(list[n].1, Source::Custom as i32);
        assert!(root.path().join(".shelf/users/a/themes/dark.json").is_file());

        delete_theme(&s, "a", "dark").await.unwrap();
        assert_eq!(listed(&s).await[dark].1, Source::BuiltIn as i32);
        assert!(matches!(delete_theme(&s, "a", "Dark").await, Err(LibraryError::Invalid(_))));
        assert!(matches!(delete_theme(&s, "a", "nope").await, Err(LibraryError::NotFound)));
    }

    #[tokio::test]
    async fn files_are_read_by_their_contents() {
        let root = tempfile::tempdir().unwrap();
        let s = storage(&root);
        let dir = root.path().join(".shelf/users/a/themes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.json"), "{").unwrap();
        std::fs::write(dir.join("bad-name.json"), r#"{"name": "a/b"}"#).unwrap();
        std::fs::write(dir.join("a.json"), r#"{"name": "Calm"}"#).unwrap();
        std::fs::write(dir.join("b.json"), r#"{"name": "calm", "layers": [{"invert": {}}]}"#)
            .unwrap();

        let list = list_themes(&s, "a").await.unwrap();
        let calm: Vec<_> = list.themes.iter().filter(|t| t.id == "calm").collect();
        assert_eq!(calm.len(), 1);
        assert_eq!(calm[0].spec.as_ref().unwrap().name, "Calm");
        assert_eq!(list.themes.len(), BUILT_IN.len() + 1);

        // Saving it leaves one file, named by its id.
        set_theme(&s, "a", spec("Calm")).await.unwrap();
        assert!(dir.join("calm.json").is_file());
        assert!(!dir.join("a.json").exists() && !dir.join("b.json").exists());
    }

    #[tokio::test]
    async fn renames_move_the_file_and_refuse_taken_names() {
        let root = tempfile::tempdir().unwrap();
        let s = storage(&root);
        set_theme(&s, "a", spec("Calm")).await.unwrap();
        set_theme(&s, "a", spec("Warm")).await.unwrap();

        let taken = rename_theme(&s, "a", "calm", "WARM").await;
        assert!(matches!(taken, Err(LibraryError::Conflict(_))));
        assert!(matches!(
            rename_theme(&s, "a", "Sepia", "Old").await,
            Err(LibraryError::Invalid(_))
        ));

        let ids = rename_theme(&s, "a", "calm", "Quiet Night").await.unwrap();
        assert_eq!(ids, ("calm".to_owned(), "quiet-night".to_owned()));
        let names: Vec<String> = listed(&s).await.into_iter().map(|(.., name)| name).collect();
        assert!(names.contains(&"Quiet Night".to_owned()) && !names.contains(&"Calm".to_owned()));

        // Only the case changes: same id, same file.
        rename_theme(&s, "a", "warm", "WARM").await.unwrap();
        assert!(listed(&s).await.iter().any(|(id, _, name)| id == "warm" && name == "WARM"));
    }

    #[tokio::test]
    async fn a_user_has_a_limited_number_of_themes() {
        let root = tempfile::tempdir().unwrap();
        let s = storage(&root);
        for i in 0..MAX_THEMES {
            set_theme(&s, "a", spec(&format!("t{i}"))).await.unwrap();
        }
        assert!(matches!(
            set_theme(&s, "a", spec("one more")).await,
            Err(LibraryError::Invalid(_))
        ));
        // Replacing one is still fine.
        set_theme(&s, "a", spec("t0")).await.unwrap();
    }
}
