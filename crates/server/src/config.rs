//! The config file at `%APPDATA%\Qapsule\config.toml`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use qapsule_core::settings::{GOAL_MINUTES, MAX_HOVER_DELAY_MS};
use qapsule_core::{AccentSource, ExpandOn, Motion, OpenOn, PageName, Settings, VisibilityMode};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, value};

pub fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("Qapsule").join("config.toml"))
}

/// The study log at `%APPDATA%\Qapsule\study.toml` (decision 0012).
pub fn study_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("Qapsule").join("study.toml"))
}

/// Saves the study log, writing beside the file and renaming over it.
pub fn save_study(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
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
    edit(path, vec![("general", "visibility", value(mode.as_str()))])
}

/// Saves Do Not Disturb, keeping the rest of the file and its comments.
pub fn save_do_not_disturb(path: &Path, on: bool) -> io::Result<()> {
    edit(path, vec![("notifications", "do_not_disturb", value(on))])
}

/// The settings the settings window shows: `[appearance]`, plus the pages, the hover
/// delay and the focus goal from their own sections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Appearance {
    pub accent_source: AccentSource,
    pub custom_accent: String,
    pub wallpaper_tint: bool,
    pub music_tint: bool,
    pub motion: Motion,
    pub translucent: bool,
    pub pages: Vec<PageName>,
    pub open_on: OpenOn,
    pub expand_on: ExpandOn,
    pub hover_delay_ms: u32,
    pub daily_goal_min: u32,
}

impl Appearance {
    pub fn of(settings: &Settings) -> Self {
        Self {
            accent_source: settings.accent_source,
            custom_accent: settings.custom_accent.clone(),
            wallpaper_tint: settings.wallpaper_tint,
            music_tint: settings.music_tint,
            motion: settings.motion,
            translucent: settings.translucent,
            pages: settings.pages.clone(),
            open_on: settings.open_on,
            expand_on: settings.expand_on,
            hover_delay_ms: settings.hover_delay_ms,
            daily_goal_min: settings.daily_goal_min,
        }
    }
}

/// A change to some of the `[appearance]` settings; fields left out stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct AppearancePatch {
    pub accent_source: Option<AccentSource>,
    pub custom_accent: Option<String>,
    pub wallpaper_tint: Option<bool>,
    pub music_tint: Option<bool>,
    pub motion: Option<Motion>,
    pub translucent: Option<bool>,
    pub pages: Option<Vec<PageName>>,
    pub open_on: Option<OpenOn>,
    pub expand_on: Option<ExpandOn>,
    pub hover_delay_ms: Option<u32>,
    pub daily_goal_min: Option<u32>,
}

impl AppearancePatch {
    /// Checks the patch. `normalise` turns colour text into the form that is stored
    /// (or `None` if it isn't a colour); the app passes the colour parser, which
    /// lives in a crate that depends on this one.
    pub fn validated(mut self, normalise: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        if let Some(text) = &self.custom_accent {
            self.custom_accent = Some(
                normalise(text).ok_or_else(|| "that isn't a colour like #60CDFF".to_string())?,
            );
        }
        if let Some(pages) = &mut self.pages {
            let mut unique: Vec<PageName> = Vec::new();
            for page in pages.iter() {
                if !unique.contains(page) {
                    unique.push(*page);
                }
            }
            if unique.is_empty() {
                return Err("keep at least one page".into());
            }
            *pages = unique;
        }
        if self
            .hover_delay_ms
            .is_some_and(|ms| ms > MAX_HOVER_DELAY_MS)
        {
            return Err(format!(
                "the hover delay is at most {MAX_HOVER_DELAY_MS} ms"
            ));
        }
        if self
            .daily_goal_min
            .is_some_and(|m| !GOAL_MINUTES.contains(&m))
        {
            return Err(format!(
                "the daily goal is from {} to {} minutes",
                GOAL_MINUTES.start(),
                GOAL_MINUTES.end()
            ));
        }
        Ok(self)
    }

    pub fn apply(&self, settings: &mut Settings) {
        if let Some(source) = self.accent_source {
            settings.accent_source = source;
        }
        if let Some(colour) = &self.custom_accent {
            settings.custom_accent = colour.clone();
        }
        if let Some(on) = self.wallpaper_tint {
            settings.wallpaper_tint = on;
        }
        if let Some(on) = self.music_tint {
            settings.music_tint = on;
        }
        if let Some(motion) = self.motion {
            settings.motion = motion;
        }
        if let Some(on) = self.translucent {
            settings.translucent = on;
        }
        if let Some(pages) = &self.pages {
            settings.pages = pages.clone();
        }
        if let Some(open_on) = self.open_on {
            settings.open_on = open_on;
        }
        if let Some(expand_on) = self.expand_on {
            settings.expand_on = expand_on;
        }
        if let Some(ms) = self.hover_delay_ms {
            settings.hover_delay_ms = ms;
        }
        if let Some(minutes) = self.daily_goal_min {
            settings.daily_goal_min = minutes;
        }
    }

    /// This patch with `newer` laid on top: the newer value of each field wins.
    pub fn then(mut self, newer: &AppearancePatch) -> Self {
        self.accent_source = newer.accent_source.or(self.accent_source);
        self.custom_accent = newer.custom_accent.clone().or(self.custom_accent);
        self.wallpaper_tint = newer.wallpaper_tint.or(self.wallpaper_tint);
        self.music_tint = newer.music_tint.or(self.music_tint);
        self.motion = newer.motion.or(self.motion);
        self.translucent = newer.translucent.or(self.translucent);
        self.pages = newer.pages.clone().or(self.pages);
        self.open_on = newer.open_on.or(self.open_on);
        self.expand_on = newer.expand_on.or(self.expand_on);
        self.hover_delay_ms = newer.hover_delay_ms.or(self.hover_delay_ms);
        self.daily_goal_min = newer.daily_goal_min.or(self.daily_goal_min);
        self
    }
}

/// Saves the `[appearance]` fields the patch sets, keeping the rest of the file.
pub fn save_appearance(path: &Path, patch: &AppearancePatch) -> io::Result<()> {
    let mut edits = Vec::new();
    if let Some(source) = patch.accent_source {
        let text = match source {
            AccentSource::Wallpaper => "wallpaper",
            AccentSource::Windows => "windows",
            AccentSource::Custom => "custom",
        };
        edits.push(("appearance", "accent_source", value(text)));
    }
    if let Some(colour) = &patch.custom_accent {
        edits.push(("appearance", "custom_accent", value(colour.as_str())));
    }
    if let Some(on) = patch.wallpaper_tint {
        edits.push(("appearance", "wallpaper_tint", value(on)));
    }
    if let Some(on) = patch.music_tint {
        edits.push(("appearance", "music_tint", value(on)));
    }
    if let Some(motion) = patch.motion {
        edits.push(("appearance", "motion", value(motion.as_str())));
    }
    if let Some(on) = patch.translucent {
        edits.push(("appearance", "translucent", value(on)));
    }
    if let Some(pages) = &patch.pages {
        let list: toml_edit::Array = pages.iter().map(|p| p.as_str()).collect();
        edits.push(("pages", "show", value(list)));
    }
    if let Some(open_on) = patch.open_on {
        edits.push(("pages", "open_on", value(open_on.as_str())));
    }
    if let Some(expand_on) = patch.expand_on {
        edits.push(("general", "expand_on", value(expand_on.as_str())));
    }
    if let Some(ms) = patch.hover_delay_ms {
        edits.push(("general", "hover_delay_ms", value(i64::from(ms))));
    }
    if let Some(minutes) = patch.daily_goal_min {
        edits.push(("focus", "daily_goal_min", value(i64::from(minutes))));
    }
    if edits.is_empty() {
        return Ok(());
    }
    edit(path, edits)
}

fn edit(path: &Path, edits: Vec<(&str, &str, Item)>) -> io::Result<()> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut doc: DocumentMut = text.parse().map_err(io::Error::other)?;
    for (section, key, mut new) in edits {
        if !doc.get(section).is_some_and(Item::is_table) {
            doc[section] = Item::Table(Table::new());
        }
        // Keep a trailing comment on the value being replaced.
        if let (Some(old), Some(new)) = (
            doc[section].get(key).and_then(Item::as_value),
            new.as_value_mut(),
        ) {
            *new.decor_mut() = old.decor().clone();
        }
        doc[section][key] = new;
    }
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pending {
    pub visibility: Option<VisibilityMode>,
    pub do_not_disturb: Option<bool>,
    pub appearance: Option<AppearancePatch>,
}

/// A change made from the tray or the settings window.
#[derive(Debug, Clone)]
pub enum Change {
    Visibility(VisibilityMode),
    DoNotDisturb(bool),
    /// Already validated.
    Appearance(AppearancePatch),
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
            Change::Appearance(ref patch) => patch.apply(&mut self.settings),
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
            Change::Appearance(patch) => {
                let all = match self.pending.appearance.take() {
                    Some(older) => older.then(&patch),
                    None => patch,
                };
                self.pending.appearance = match save_appearance(path, &all) {
                    Ok(()) => None,
                    Err(_) => {
                        save_failed("the appearance settings");
                        Some(all)
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
        if let Some(patch) = self.pending.appearance.take() {
            patch.apply(&mut new);
            if path.is_some_and(|p| save_appearance(p, &patch).is_err()) {
                save_failed("the appearance settings");
                self.pending.appearance = Some(patch);
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

    fn fake_normalise(text: &str) -> Option<String> {
        let digits = text.trim().strip_prefix('#')?;
        (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| format!("#{}", digits.to_ascii_lowercase()))
    }

    #[test]
    fn a_bad_colour_is_rejected_and_a_good_one_is_normalised() {
        let bad = AppearancePatch {
            custom_accent: Some("nope".into()),
            ..Default::default()
        };
        assert!(bad.validated(fake_normalise).is_err());
        let good = AppearancePatch {
            custom_accent: Some(" #60CDFF ".into()),
            ..Default::default()
        };
        let good = good.validated(fake_normalise).unwrap();
        assert_eq!(good.custom_accent.as_deref(), Some("#60cdff"));
    }

    #[test]
    fn a_partial_update_changes_only_what_it_names() {
        let mut settings = Settings::default();
        let before = Appearance::of(&settings);
        AppearancePatch {
            music_tint: Some(false),
            accent_source: Some(AccentSource::Custom),
            ..Default::default()
        }
        .apply(&mut settings);
        let after = Appearance::of(&settings);
        assert_eq!(after.accent_source, AccentSource::Custom);
        assert!(!after.music_tint);
        assert_eq!(after.custom_accent, before.custom_accent);
        assert_eq!(after.wallpaper_tint, before.wallpaper_tint);

        let merged = AppearancePatch {
            music_tint: Some(true),
            wallpaper_tint: Some(false),
            ..Default::default()
        }
        .then(&AppearancePatch {
            music_tint: Some(false),
            ..Default::default()
        });
        assert_eq!(merged.music_tint, Some(false));
        assert_eq!(merged.wallpaper_tint, Some(false));
        assert_eq!(merged.accent_source, None);
    }

    #[test]
    fn saving_appearance_keeps_comments_order_and_other_sections() {
        let dir = std::env::temp_dir().join(format!("qapsule-config-ap-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let original = "# mine
[general]
visibility = \"always\" # keep

[appearance]
accent_source = \"wallpaper\" # where from
music_tint = true
unknown = 1

[peek]
dual_status = false
";
        fs::write(&path, original).unwrap();
        save_appearance(
            &path,
            &AppearancePatch {
                accent_source: Some(AccentSource::Custom),
                custom_accent: Some("#112233".into()),
                music_tint: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        for kept in [
            "# mine",
            "# keep",
            "# where from",
            "unknown = 1",
            "dual_status = false",
        ] {
            assert!(text.contains(kept), "{kept} lost:\n{text}");
        }
        assert!(text.find("[general]") < text.find("[appearance]"));
        assert!(text.find("[appearance]") < text.find("[peek]"));
        let (settings, warnings) = load(&path);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.accent_source, AccentSource::Custom);
        assert_eq!(settings.custom_accent, "#112233");
        assert!(!settings.music_tint && settings.wallpaper_tint);
        assert_eq!(settings.visibility, VisibilityMode::Always);
        assert!(!settings.dual_status);
        assert!(!dir.join("config.toml.tmp").exists());

        // A missing file gets the section.
        let fresh = dir.join("fresh.toml");
        save_appearance(
            &fresh,
            &AppearancePatch {
                wallpaper_tint: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!load(&fresh).0.wallpaper_tint);
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
