//! The config file at `%APPDATA%\Qapsule\config.toml`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use qapsule_core::{Settings, VisibilityMode};
use toml_edit::{DocumentMut, Item, Table, value};

pub fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("Qapsule").join("config.toml"))
}

/// Reads settings. A missing file means defaults; mistakes come back as warnings.
pub fn load(path: &Path) -> (Settings, Vec<String>) {
    match fs::read_to_string(path) {
        Ok(text) => Settings::from_toml(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Settings::default(), Vec::new()),
        Err(e) => (
            Settings::default(),
            vec![format!("couldn't read {}: {e}", path.display())],
        ),
    }
}

/// Saves the visibility mode, keeping the rest of the file and its comments.
pub fn save_visibility(path: &Path, mode: VisibilityMode) -> io::Result<()> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut doc: DocumentMut = text.parse().map_err(io::Error::other)?;
    if !doc.get("general").is_some_and(Item::is_table) {
        doc["general"] = Item::Table(Table::new());
    }
    doc["general"]["visibility"] = value(mode.as_str());
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_the_mode_keeps_comments_and_other_settings() {
        let dir = std::env::temp_dir().join(format!("qapsule-config-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(
            load(&path).0,
            Settings::default(),
            "a missing file means defaults"
        );
        save_visibility(&path, VisibilityMode::OnDemand).unwrap();
        assert_eq!(load(&path).0.visibility, VisibilityMode::OnDemand);

        fs::write(
            &path,
            "# mine\n[general]\nvisibility = \"auto\" # hi\n[peek]\ndual_status = false\n",
        )
        .unwrap();
        save_visibility(&path, VisibilityMode::Always).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mine"), "{text}");
        let (settings, warnings) = load(&path);
        assert!(warnings.is_empty());
        assert_eq!(settings.visibility, VisibilityMode::Always);
        assert!(!settings.dual_status);
        let _ = fs::remove_dir_all(&dir);
    }
}
