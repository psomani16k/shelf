use anyhow::anyhow;
use shelf_proto::{
    api::{AddAnnotationRequest, UpdateAnnotationRequest},
    bookkeeping::{Annotation, highlight_color},
};
use shelf_utility::timeutil::Time;
use tracing::warn;

use crate::library::LibraryError;

const MAX_CFI_CHARS: usize = 2_000;
const MAX_TEXT_CHARS: usize = 2_000;
const MAX_CHAPTER_CHARS: usize = 500;
const MAX_NOTE_CHARS: usize = 10_000;
/// Per book and user.
const MAX_ANNOTATIONS: usize = 10_000;

/// 32 lowercase hex characters.
fn new_id() -> anyhow::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("couldn't get random bytes: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn is_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// What's wrong with the annotation, if anything.
fn problem(a: &Annotation) -> Option<&'static str> {
    let chars = |s: &str| s.chars().count();
    [
        (highlight_color::Value::try_from(a.color).is_err(), "unknown highlight colour"),
        (!a.cfi.starts_with("epubcfi(") || chars(&a.cfi) > MAX_CFI_CHARS, "invalid CFI"),
        (chars(&a.text) > MAX_TEXT_CHARS, "the text is too long"),
        (chars(&a.chapter) > MAX_CHAPTER_CHARS, "the chapter name is too long"),
        (chars(&a.note) > MAX_NOTE_CHARS, "the note is too long"),
    ]
    .into_iter()
    .find_map(|(bad, why)| bad.then_some(why))
}

fn validate(a: &Annotation) -> Result<(), LibraryError> {
    problem(a).map_or(Ok(()), |why| Err(LibraryError::Invalid(why.to_owned())))
}

fn too_many() -> String {
    format!("a book can have at most {MAX_ANNOTATIONS} annotations")
}

/// Adds the annotation with a new id and returns it as stored.
pub fn add(
    list: &mut Vec<Annotation>,
    req: AddAnnotationRequest,
    now: Time,
) -> Result<Annotation, LibraryError> {
    if list.len() >= MAX_ANNOTATIONS {
        return Err(LibraryError::Invalid(too_many()));
    }
    let now = now.to_string();
    let mut annotation = Annotation {
        id: String::new(),
        cfi: req.cfi,
        text: req.text,
        chapter: req.chapter,
        note: req.note,
        color: req.color,
        created: now.clone(),
        updated: now,
    };
    validate(&annotation)?;
    annotation.id = new_id()?;
    list.push(annotation.clone());
    Ok(annotation)
}

/// Changes the fields `req` sets and returns the annotation as stored.
pub fn update(
    list: &mut [Annotation],
    req: UpdateAnnotationRequest,
    now: Time,
) -> Result<Annotation, LibraryError> {
    let stored = list
        .iter_mut()
        .find(|a| a.id == req.annotation_id)
        .ok_or(LibraryError::NotFound)?;
    let mut changed = stored.clone();
    if let Some(note) = req.note {
        changed.note = note;
    }
    if let Some(color) = req.color {
        changed.color = color;
    }
    validate(&changed)?;
    changed.updated = now.to_string();
    *stored = changed.clone();
    Ok(changed)
}

/// Whether there was one to remove.
pub fn remove(list: &mut Vec<Annotation>, id: &str) -> bool {
    let before = list.len();
    list.retain(|a| a.id != id);
    list.len() != before
}

/// Adds the imported annotations, each replacing the one with its id if it was updated later.
/// Returns how many were added or replaced; invalid ones are skipped.
pub fn merge(list: &mut Vec<Annotation>, imported: Vec<Annotation>) -> usize {
    let mut written = 0;
    for annotation in imported {
        let (annotation, updated) = match checked_import(annotation) {
            Ok(checked) => checked,
            Err(why) => {
                warn!(%why, "skipped an imported annotation");
                continue;
            }
        };
        match list.iter().position(|a| a.id == annotation.id) {
            Some(i) => {
                let newer = list[i].updated.parse::<Time>().is_ok_and(|ours| updated > ours);
                if newer {
                    list[i] = annotation;
                    written += 1;
                }
            }
            None if list.len() < MAX_ANNOTATIONS => {
                list.push(annotation);
                written += 1;
            }
            None => warn!(why = %too_many(), "skipped an imported annotation"),
        }
    }
    // Timestamps are all `Time`'s format now, so they sort as strings.
    list.sort_by(|a, b| a.created.cmp(&b.created));
    written
}

/// The annotation with its timestamps rewritten in `Time`'s format, and when it was updated.
fn checked_import(mut a: Annotation) -> Result<(Annotation, Time), String> {
    if let Some(why) = problem(&a) {
        return Err(why.to_owned());
    }
    if !is_id(&a.id) {
        return Err(format!("invalid annotation id {:?}", a.id));
    }
    let created: Time = a.created.parse().map_err(|e| format!("{e:#}"))?;
    let updated: Time = a.updated.parse().map_err(|e| format!("{e:#}"))?;
    a.created = created.to_string();
    a.updated = updated.to_string();
    Ok((a, updated))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn at(secs: u64) -> Time {
        Time::from(UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs))
    }

    fn highlight(cfi: &str) -> AddAnnotationRequest {
        AddAnnotationRequest {
            cfi: cfi.into(),
            text: "some text".into(),
            chapter: "One".into(),
            color: highlight_color::Value::Green as i32,
            ..Default::default()
        }
    }

    fn edit(id: &str, note: Option<&str>, color: Option<highlight_color::Value>) -> UpdateAnnotationRequest {
        UpdateAnnotationRequest {
            annotation_id: id.into(),
            note: note.map(Into::into),
            color: color.map(|c| c as i32),
            ..Default::default()
        }
    }

    #[test]
    fn add_stamps_an_id_and_times() {
        let mut list = Vec::new();
        let a = add(&mut list, highlight("epubcfi(/6/2!/4/2,/1:0,/1:4)"), at(0)).unwrap();
        let b = add(&mut list, highlight("epubcfi(/6/2!/4/4)"), at(1)).unwrap();

        assert!(is_id(&a.id) && is_id(&b.id) && a.id != b.id);
        assert_eq!(a.created, at(0).to_string());
        assert_eq!(a.updated, a.created);
        assert_eq!(a.text, "some text");
        assert_eq!(list, [a, b]);
    }

    #[test]
    fn add_refuses_invalid_annotations() {
        let long = |n| "x".repeat(n);
        let bad = [
            AddAnnotationRequest { color: 9, ..highlight("epubcfi(/6/2)") },
            highlight(""),
            highlight("/6/2"),
            highlight(&format!("epubcfi({})", long(MAX_CFI_CHARS))),
            AddAnnotationRequest { text: long(MAX_TEXT_CHARS + 1), ..highlight("epubcfi(/6/2)") },
            AddAnnotationRequest { chapter: long(MAX_CHAPTER_CHARS + 1), ..highlight("epubcfi(/6/2)") },
            AddAnnotationRequest { note: long(MAX_NOTE_CHARS + 1), ..highlight("epubcfi(/6/2)") },
        ];
        for req in bad {
            let mut list = Vec::new();
            assert!(matches!(add(&mut list, req, at(0)), Err(LibraryError::Invalid(_))));
            assert!(list.is_empty());
        }
        let mut list = Vec::new();
        let note = AddAnnotationRequest { note: "é".repeat(MAX_NOTE_CHARS), ..highlight("epubcfi(/6/2)") };
        assert!(add(&mut list, note, at(0)).is_ok(), "the limit counts characters, not bytes");
    }

    #[test]
    fn add_stops_at_the_limit() {
        let mut list = Vec::new();
        let first = add(&mut list, highlight("epubcfi(/6/2)"), at(0)).unwrap();
        list.resize(MAX_ANNOTATIONS, first);
        assert!(matches!(add(&mut list, highlight("epubcfi(/6/2)"), at(0)), Err(LibraryError::Invalid(_))));
    }

    #[test]
    fn update_changes_only_the_note_and_colour() {
        let mut list = Vec::new();
        let a = add(&mut list, highlight("epubcfi(/6/2)"), at(0)).unwrap();

        let noted = update(&mut list, edit(&a.id, Some("why"), None), at(5)).unwrap();
        assert_eq!((noted.note.as_str(), noted.color), ("why", a.color));
        assert_eq!(noted.updated, at(5).to_string());
        assert_eq!(noted.created, a.created);

        let pink = update(&mut list, edit(&a.id, None, Some(highlight_color::Value::Pink)), at(6)).unwrap();
        assert_eq!((pink.note.as_str(), pink.color), ("why", highlight_color::Value::Pink as i32));
        assert_eq!(list, [pink.clone()]);

        let cleared = update(&mut list, edit(&a.id, Some(""), None), at(7)).unwrap();
        assert_eq!(cleared.note, "");
        assert_eq!((cleared.cfi, cleared.text), (a.cfi, a.text));
    }

    #[test]
    fn update_of_an_unknown_or_invalid_change_is_refused() {
        let mut list = Vec::new();
        let a = add(&mut list, highlight("epubcfi(/6/2)"), at(0)).unwrap();

        assert!(matches!(update(&mut list, edit("gone", Some("x"), None), at(1)), Err(LibraryError::NotFound)));
        let mut bad = edit(&a.id, None, None);
        bad.color = Some(9);
        assert!(matches!(update(&mut list, bad, at(1)), Err(LibraryError::Invalid(_))));
        assert_eq!(list, [a]);
    }

    #[test]
    fn remove_is_idempotent() {
        let mut list = Vec::new();
        let a = add(&mut list, highlight("epubcfi(/6/2)"), at(0)).unwrap();
        assert!(remove(&mut list, &a.id));
        assert!(!remove(&mut list, &a.id));
        assert!(list.is_empty());
    }

    #[test]
    fn merge_keeps_the_newer_copy_of_each() {
        let mut theirs = Vec::new();
        let shared = add(&mut theirs, highlight("epubcfi(/6/2)"), at(0)).unwrap();
        let only_theirs = add(&mut theirs, highlight("epubcfi(/6/4)"), at(1)).unwrap();
        let mut own = vec![shared.clone()];
        let only_own = add(&mut own, highlight("epubcfi(/6/6)"), at(2)).unwrap();

        // Ours is newer: it stays; theirs adds the one we don't have.
        let ours = update(&mut own, edit(&shared.id, Some("mine"), None), at(10)).unwrap();
        update(&mut theirs, edit(&shared.id, Some("theirs"), None), at(9)).unwrap();
        assert_eq!(merge(&mut own, theirs.clone()), 1);
        assert_eq!(own, [ours, only_theirs.clone(), only_own.clone()]);

        // Theirs is newer now: it replaces ours.
        let newer = update(&mut theirs, edit(&shared.id, Some("theirs, later"), None), at(11)).unwrap();
        assert_eq!(merge(&mut own, theirs.clone()), 1);
        assert_eq!(own, [newer, only_theirs, only_own]);

        // Importing the same again changes nothing.
        let before = own.clone();
        assert_eq!(merge(&mut own, theirs), 0);
        assert_eq!(own, before);
    }

    #[test]
    fn merge_skips_invalid_imports_and_normalises_times() {
        let mut theirs = Vec::new();
        let good = add(&mut theirs, highlight("epubcfi(/6/2)"), at(0)).unwrap();
        let offset = Annotation {
            id: "0".repeat(32),
            created: "2023-11-14T22:13:19+01:00".into(),
            updated: "2023-11-14T22:13:19.5+01:00".into(),
            ..good.clone()
        };
        let bad = [
            Annotation { id: "../x".into(), ..good.clone() },
            Annotation { id: "A".repeat(32), ..good.clone() },
            Annotation { id: "1".repeat(32), updated: "yesterday".into(), ..good.clone() },
            Annotation { id: "2".repeat(32), cfi: "nope".into(), ..good.clone() },
        ];
        let mut own = Vec::new();
        let imported = [vec![good.clone(), offset], bad.to_vec()].concat();

        assert_eq!(merge(&mut own, imported), 2);
        assert_eq!(own.len(), 2);
        // Sorted oldest first.
        assert_eq!(own[0].created, "2023-11-14T21:13:19Z");
        assert_eq!(own[0].updated, "2023-11-14T21:13:19Z");
        assert_eq!(own[1], good);
    }
}
