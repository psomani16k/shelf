use std::{io::ErrorKind, path::Path};

use serde_json::{Map, Value};
use shelf_proto::config::Config;
use tokio::fs;
use tracing::warn;

use crate::state::write_atomic;

const CONFIG_FILE: &str = ".shelf/config.json";

/// A missing or unparseable `config.json` gives the default config.
pub async fn read_config(root: &Path) -> anyhow::Result<Config> {
    let path = root.join(CONFIG_FILE);
    let json = match fs::read_to_string(&path).await {
        Ok(json) => json,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e.into()),
    };
    Ok(serde_json::from_str(&json).unwrap_or_else(|e| {
        warn!(path = ?path, error = %e, "unparseable config, using the default");
        Config::default()
    }))
}

/// Merges the fields set in `patch` into the stored file and returns the full config.
/// The merge works on raw JSON, so fields this build doesn't know about survive.
pub async fn update_config(root: &Path, patch: &Config) -> anyhow::Result<Config> {
    let path = root.join(CONFIG_FILE);
    let mut stored = match fs::read_to_string(&path).await {
        Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
            warn!(path = ?path, error = %e, "unparseable config, starting from empty");
            Value::Object(Map::new())
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => Value::Object(Map::new()),
        Err(e) => return Err(e.into()),
    };
    if !stored.is_object() {
        warn!(path = ?path, "config isn't a JSON object, starting from empty");
        stored = Value::Object(Map::new());
    }

    merge(&mut stored, serde_json::to_value(patch)?);
    let config = serde_json::from_value(stored.clone())?;
    write_atomic(&path, serde_json::to_string_pretty(&stored)?.as_bytes()).await?;
    Ok(config)
}

/// Objects merge key by key; anything else, lists included, replaces what's stored.
fn merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, patch) => *base = patch,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn merge_changes_only_patched_fields() {
        let mut base = json!({"a": 1, "b": {"c": 2, "d": 3}, "e": [1, 2]});
        merge(&mut base, json!({"b": {"c": 5}, "e": [9]}));
        assert_eq!(base, json!({"a": 1, "b": {"c": 5, "d": 3}, "e": [9]}));
    }

    #[tokio::test]
    async fn missing_config_is_default() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read_config(root.path()).await.unwrap(), Config::default());
    }

    #[tokio::test]
    async fn update_changes_only_sent_fields() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".shelf")).await.unwrap();
        let patch = |json| serde_json::from_value::<Config>(json).unwrap();

        update_config(
            root.path(),
            &patch(json!({"reader": {"flow": "PAGINATED", "theme": "DARK"}})),
        )
        .await
        .unwrap();
        // SCROLLED is the zero value: `optional` is what lets it through.
        let config = update_config(root.path(), &patch(json!({"reader": {"flow": "SCROLLED"}})))
            .await
            .unwrap();

        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            json!({"reader": {"flow": "SCROLLED", "theme": "DARK"}})
        );
    }

    #[tokio::test]
    async fn update_keeps_unknown_fields() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(CONFIG_FILE);
        fs::create_dir(path.parent().unwrap()).await.unwrap();
        fs::write(&path, r#"{"future": 1}"#).await.unwrap();

        update_config(root.path(), &Config::default())
            .await
            .unwrap();

        let stored: Value =
            serde_json::from_str(&fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(stored, json!({"future": 1}));
    }
}
