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

/// Reads settings again after the file changed. Unlike `load`, a file that can't
/// be read, is missing, is empty or isn't valid TOML is an error (a short fixed
/// reason, no content), so a half-saved file never resets the settings.
pub fn reload(path: &Path) -> Result<(Settings, Vec<String>), String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err("the file is missing".into()),
        Err(_) => return Err("couldn't read the file".into()),
    };
    if text.trim().is_empty() {
        return Err("the file is empty".into());
    }
    if text.parse::<DocumentMut>().is_err() {
        return Err("not valid TOML".into());
    }
    Ok(Settings::from_toml(&text))
}

/// Saves the visibility mode, keeping the rest of the file and its comments.
pub fn save_visibility(path: &Path, mode: VisibilityMode) -> io::Result<()> {
    edit(path, "general", "visibility", value(mode.as_str()))
}

/// Saves Do Not Disturb, keeping the rest of the file and its comments.
pub fn save_do_not_disturb(path: &Path, on: bool) -> io::Result<()> {
    edit(path, "notifications", "do_not_disturb", value(on))
}

fn edit(path: &Path, section: &str, key: &str, new: Item) -> io::Result<()> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut doc: DocumentMut = text.parse().map_err(io::Error::other)?;
    if !doc.get(section).is_some_and(Item::is_table) {
        doc[section] = Item::Table(Table::new());
    }
    let mut new = new;
    // Keep a trailing comment on the value being replaced.
    if let (Some(old), Some(new)) = (
        doc[section].get(key).and_then(Item::as_value),
        new.as_value_mut(),
    ) {
        *new.decor_mut() = old.decor().clone();
    }
    doc[section][key] = new;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Write beside the file, then rename over it, so a reader (or the watcher)
    // never sees a half-written config.
    let mut tmp_name = path.file_name().unwrap_or_default().to_owned();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    fs::write(&tmp, doc.to_string())?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// A tray change that could not be saved because the file is invalid. The latest
/// value of each wins; it is applied on top of the next good reload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pending {
    pub visibility: Option<VisibilityMode>,
    pub do_not_disturb: Option<bool>,
}

/// A change made from the tray.
#[derive(Debug, Clone, Copy)]
pub enum Change {
    Visibility(VisibilityMode),
    DoNotDisturb(bool),
}

/// The live settings plus unsaved tray edits. The app keeps this behind one lock
/// and calls these methods with the lock held, passing `apply`, which sends the
/// settings to the engine and providers. Because `apply` runs under the same lock
/// that orders the changes, a reload racing a tray click cannot leave the engine
/// with older settings than memory.
#[derive(Debug, Default)]
pub struct Live {
    pub settings: Settings,
    pub pending: Pending,
}

fn save_failed(what: &str) {
    eprintln!("qapsule: couldn't save {what}; will retry when the file is valid");
}

impl Live {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            pending: Pending::default(),
        }
    }

    /// A tray click: always takes effect; if saving fails the edit is remembered.
    pub fn change(
        &mut self,
        change: Change,
        path: Option<&Path>,
        apply: &mut dyn FnMut(&Settings),
    ) {
        match change {
            Change::Visibility(mode) => self.settings.visibility = mode,
            Change::DoNotDisturb(on) => self.settings.do_not_disturb = on,
        }
        apply(&self.settings);
        let Some(path) = path else { return };
        match change {
            Change::Visibility(mode) => {
                self.pending.visibility = match save_visibility(path, mode) {
                    Ok(()) => None,
                    Err(_) => {
                        save_failed("the visibility mode");
                        Some(mode)
                    }
                };
            }
            Change::DoNotDisturb(on) => {
                self.pending.do_not_disturb = match save_do_not_disturb(path, on) {
                    Ok(()) => None,
                    Err(_) => {
                        save_failed("Do Not Disturb");
                        Some(on)
                    }
                };
            }
        }
    }

    /// The file was reloaded. Pending tray edits go on top and are saved; the result
    /// is applied if it differs from what is live. Returns whether it was applied.
    pub fn reloaded(
        &mut self,
        mut new: Settings,
        path: Option<&Path>,
        apply: &mut dyn FnMut(&Settings),
    ) -> bool {
        if let Some(mode) = self.pending.visibility {
            new.visibility = mode;
            if path.is_none_or(|p| save_visibility(p, mode).is_ok()) {
                self.pending.visibility = None;
            } else {
                save_failed("the visibility mode");
            }
        }
        if let Some(on) = self.pending.do_not_disturb {
            new.do_not_disturb = on;
            if path.is_none_or(|p| save_do_not_disturb(p, on).is_ok()) {
                self.pending.do_not_disturb = None;
            } else {
                save_failed("Do Not Disturb");
            }
        }
        if self.settings == new {
            return false;
        }
        self.settings = new;
        apply(&self.settings);
        true
    }
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

    #[test]
    fn saving_do_not_disturb_keeps_comments_and_creates_the_section() {
        let dir = std::env::temp_dir().join(format!("qapsule-config-dnd-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);

        save_do_not_disturb(&path, true).unwrap();
        assert!(load(&path).0.do_not_disturb);

        fs::write(
            &path,
            "# mine
[notifications]
do_not_disturb = true # hi
calendar = { pop = false }
[general]
visibility = \"always\"
",
        )
        .unwrap();
        save_do_not_disturb(&path, false).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# mine") && text.contains("# hi"), "{text}");
        let (settings, warnings) = load(&path);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!settings.do_not_disturb);
        assert_eq!(settings.visibility, VisibilityMode::Always);
        assert!(!settings.pop_setting("calendar:events").pop);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reload_refuses_a_broken_empty_or_missing_file() {
        let dir = std::env::temp_dir().join(format!("qapsule-config-rl-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(reload(&path).unwrap_err(), "the file is missing");
        assert_eq!(load(&path).0, Settings::default(), "startup still defaults");

        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "[notifications]\ndo_not_disturb = true\n").unwrap();
        assert!(reload(&path).unwrap().0.do_not_disturb);

        fs::write(&path, "  \n\t\n").unwrap();
        assert_eq!(reload(&path).unwrap_err(), "the file is empty");

        fs::write(&path, "[notifications\ndo_not_disturb = tru").unwrap();
        let reason = reload(&path).unwrap_err();
        assert_eq!(reason, "not valid TOML");
        assert!(!reason.contains("tru"));

        // A directory where the file should be can't be read.
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(reload(&path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_are_atomic_and_leave_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("qapsule-config-at-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);
        save_do_not_disturb(&path, true).unwrap();
        save_visibility(&path, VisibilityMode::Always).unwrap();
        assert!(!dir.join("config.toml.tmp").exists());
        assert!(reload(&path).unwrap().0.do_not_disturb);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tray_edits_while_the_file_is_invalid_are_kept_and_merged_after_the_next_good_reload() {
        let dir = std::env::temp_dir().join(format!("qapsule-config-pe-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "[notifications\n").unwrap();

        let mut live = Live::new(Settings::default());
        let applied = std::cell::Cell::new(0);
        let mut apply = |_: &Settings| applied.set(applied.get() + 1);
        live.change(Change::DoNotDisturb(true), Some(&path), &mut apply);
        live.change(
            Change::Visibility(VisibilityMode::Always),
            Some(&path),
            &mut apply,
        );
        live.change(
            Change::Visibility(VisibilityMode::OnDemand),
            Some(&path),
            &mut apply,
        );
        assert!(live.settings.do_not_disturb, "the click took effect");
        assert_eq!(live.pending.visibility, Some(VisibilityMode::OnDemand));
        assert_eq!(live.pending.do_not_disturb, Some(true));

        // The user fixes the file; it says something else.
        fs::write(
            &path,
            "# mine\n[notifications]\ndo_not_disturb = false\n[peek]\ndual_status = false\n",
        )
        .unwrap();
        let (reloaded, _) = reload(&path).unwrap();
        assert!(live.reloaded(reloaded, Some(&path), &mut apply));
        assert_eq!(live.pending, Pending::default());
        assert!(live.settings.do_not_disturb && !live.settings.dual_status);
        assert_eq!(live.settings.visibility, VisibilityMode::OnDemand);
        let (saved, _) = reload(&path).unwrap();
        assert_eq!(saved, live.settings, "the edits reached the file");
        assert!(fs::read_to_string(&path).unwrap().contains("# mine"));

        // Our own save coming back changes nothing.
        let before = applied.get();
        let (again, _) = reload(&path).unwrap();
        assert!(!live.reloaded(again, Some(&path), &mut apply));
        assert_eq!(applied.get(), before);
        let _ = fs::remove_dir_all(&dir);
    }
}
