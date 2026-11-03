//! `qapsule install|uninstall claude-code`: merge the hook template into
//! Claude Code's `settings.json`, or take it out again.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// The one source of truth for which events qapsule hooks.
const TEMPLATE: &str = include_str!("../../../integrations/claude-code/settings.json");

/// `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`.
pub fn default_settings_path() -> Result<PathBuf, String> {
    if let Some(dir) = env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir).join("settings.json"));
    }
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .filter(|h| !h.is_empty())
        .ok_or("cannot find your home folder; pass --settings <path>")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

fn template_hooks() -> Map<String, Value> {
    let template: Value = serde_json::from_str(TEMPLATE).expect("the embedded template is JSON");
    match template.get("hooks") {
        Some(Value::Object(hooks)) => hooks.clone(),
        _ => unreachable!("the template has a hooks object"),
    }
}

/// Is this one hook item qapsule's (`qapsule hook claude-code ...`)?
fn is_ours(item: &Value) -> bool {
    item.get("command").and_then(Value::as_str) == Some("qapsule")
        && item.get("args").and_then(Value::as_array).is_some_and(|a| {
            a.first().and_then(Value::as_str) == Some("hook")
                && a.get(1).and_then(Value::as_str) == Some("claude-code")
        })
}

fn entry_has_ours(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(is_ours))
}

fn hooks_object(
    settings: &mut Value,
    create: bool,
) -> Result<Option<&mut Map<String, Value>>, String> {
    let root = settings
        .as_object_mut()
        .ok_or("settings.json must hold a JSON object")?;
    if create {
        root.entry("hooks")
            .or_insert_with(|| Value::Object(Map::new()));
    }
    match root.get_mut("hooks") {
        None => Ok(None),
        Some(Value::Object(hooks)) => Ok(Some(hooks)),
        Some(_) => Err("\"hooks\" in settings.json is not an object".into()),
    }
}

/// Adds the template's entries; returns the events that changed.
pub fn add_hooks(settings: &mut Value) -> Result<Vec<String>, String> {
    let template = template_hooks();
    let hooks = hooks_object(settings, true)?.expect("created above");
    let mut added = Vec::new();
    for (event, entries) in template {
        let slot = hooks
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        let list = slot
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{event} in settings.json is not an array"))?;
        if list.iter().any(entry_has_ours) {
            continue;
        }
        list.extend(entries.as_array().cloned().unwrap_or_default());
        added.push(event);
    }
    // Nothing added to a freshly created, empty block: leave no trace.
    if hooks.is_empty() {
        settings.as_object_mut().expect("object").remove("hooks");
    }
    Ok(added)
}

/// Removes only qapsule's hooks; returns the events that changed.
pub fn remove_hooks(settings: &mut Value) -> Result<Vec<String>, String> {
    let Some(hooks) = hooks_object(settings, false)? else {
        return Ok(Vec::new());
    };
    let mut removed = Vec::new();
    for (event, entries) in hooks.iter_mut() {
        let Some(list) = entries.as_array_mut() else {
            continue;
        };
        let mut touched = false;
        for entry in list.iter_mut() {
            if let Some(items) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
                let n = items.len();
                items.retain(|i| !is_ours(i));
                touched |= items.len() != n;
            }
        }
        // Drop only entries we emptied, never ones the user left empty.
        if touched {
            list.retain(|e| {
                e.get("hooks")
                    .and_then(Value::as_array)
                    .is_none_or(|i| !i.is_empty())
            });
            removed.push(event.clone());
        }
    }
    // Drop event arrays we emptied, and the hooks block if nothing is left.
    hooks.retain(|k, v| !(removed.contains(k) && v.as_array().is_some_and(Vec::is_empty)));
    if hooks.is_empty() {
        settings.as_object_mut().expect("object").remove("hooks");
    }
    Ok(removed)
}

fn read(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(Value::Object(Map::new())),
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); fix it first, nothing was changed",
                path.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

fn write(path: &Path, value: &Value) -> Result<(), String> {
    let fail = |what: &str, e: std::io::Error| format!("{what} {}: {e}", path.display());
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| fail("cannot create the folder for", e))?;
    }
    if path.exists() {
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak");
        fs::copy(path, &bak).map_err(|e| fail("cannot back up", e))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');
    fs::write(&tmp, text).map_err(|e| fail("cannot write", e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        fail("cannot replace", e)
    })
}

const NEW_SESSIONS: &str = "Hooks apply to new Claude Code sessions; restart any running ones.";

pub fn install(path: &Path) -> Result<String, String> {
    let mut settings = read(path)?;
    let added = add_hooks(&mut settings)?;
    if added.is_empty() {
        return Ok(format!("Already installed in {}", path.display()));
    }
    write(path, &settings)?;
    Ok(format!(
        "Added hooks for {} to {}\n{NEW_SESSIONS}",
        added.join(", "),
        path.display()
    ))
}

pub fn uninstall(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok(format!(
            "Nothing to remove: {} does not exist",
            path.display()
        ));
    }
    let mut settings = read(path)?;
    let removed = remove_hooks(&mut settings)?;
    if removed.is_empty() {
        return Ok(format!("Nothing to remove from {}", path.display()));
    }
    write(path, &settings)?;
    Ok(format!(
        "Removed hooks for {} from {}\n{NEW_SESSIONS}",
        removed.join(", "),
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn events(v: &Value) -> Vec<String> {
        v["hooks"].as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn installs_into_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("settings.json");
        let msg = install(&path).unwrap();
        assert!(msg.contains("new Claude Code sessions"));
        let v: Value = serde_json::from_str(&text(&path)).unwrap();
        let template = template_hooks();
        assert_eq!(events(&v), template.keys().cloned().collect::<Vec<_>>());
        assert!(events(&v).contains(&"SubagentStart".to_string()));
        assert!(text(&path).starts_with("{\n  \"hooks\""));
        assert!(!path.with_file_name("settings.json.bak").exists());
    }

    #[test]
    fn installs_alongside_user_settings_and_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mine = json!({"hooks": [{"type": "command", "command": "echo"}]});
        let original = json!({
            "model": "opus",
            "hooks": {"Stop": [mine.clone()], "PreToolUse": [mine.clone()]},
            "zzz": 1
        });
        fs::write(&path, original.to_string()).unwrap();
        install(&path).unwrap();
        let v: Value = serde_json::from_str(&text(&path)).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["hooks"]["PreToolUse"], json!([mine.clone()]));
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(v["hooks"]["Stop"][0], mine);
        let keys: Vec<_> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "hooks", "zzz"]);
        assert_eq!(events(&v)[..2], ["Stop", "PreToolUse"]);
        let bak: Value =
            serde_json::from_str(&text(&dir.path().join("settings.json.bak"))).unwrap();
        assert_eq!(bak, original);
    }

    #[test]
    fn installing_twice_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        install(&path).unwrap();
        let first = text(&path);
        let msg = install(&path).unwrap();
        assert!(msg.contains("Already installed"));
        assert_eq!(text(&path), first);
        assert!(!dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn invalid_json_is_refused_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ not json").unwrap();
        let err = install(&path).unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
        assert_eq!(text(&path), "{ not json");
        assert!(uninstall(&path).is_err());
        assert!(!dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn uninstall_removes_only_ours() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mine = json!({"hooks": [{"type": "command", "command": "echo"}]});
        let original = json!({"model": "opus", "hooks": {"Stop": [mine]}});
        fs::write(&path, original.to_string()).unwrap();
        install(&path).unwrap();
        uninstall(&path).unwrap();
        let v: Value = serde_json::from_str(&text(&path)).unwrap();
        assert_eq!(v, original);
        assert!(uninstall(&path).unwrap().contains("Nothing to remove"));
    }

    #[test]
    fn uninstall_drops_an_empty_hooks_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"model":"opus"}"#).unwrap();
        install(&path).unwrap();
        uninstall(&path).unwrap();
        let v: Value = serde_json::from_str(&text(&path)).unwrap();
        assert_eq!(v, json!({"model": "opus"}));
    }

    #[test]
    fn a_shared_entry_keeps_the_users_hook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let ours = template_hooks()["Stop"][0]["hooks"][0].clone();
        let shared = json!({"hooks": [{"type": "command", "command": "echo"}, ours]});
        fs::write(&path, json!({"hooks": {"Stop": [shared]}}).to_string()).unwrap();
        uninstall(&path).unwrap();
        let v: Value = serde_json::from_str(&text(&path)).unwrap();
        assert_eq!(
            v,
            json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "echo"}]}]}})
        );
    }
}
