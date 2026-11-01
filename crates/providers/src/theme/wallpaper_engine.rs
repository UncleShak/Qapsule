//! Wallpaper Engine's current wallpaper, read from its `config.json` (format notes in
//! `docs/integrations/wallpaper-engine.md`). Pure parsing and path logic; the only
//! file access is in [`preview_image`], which reads the project folder.
//!
//! The format is undocumented. Every function here fails with a [`Failure`] that
//! says why, so the caller can log it and fall through to the next source.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

/// Why the preview image could not be found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    NotJson,
    /// No user in the file has `general.wallpaperconfig.selectedwallpapers`.
    NoWallpaperConfig,
    /// `selectedwallpapers` has no entry with a usable `file`.
    NoSelection,
    /// None of the selected wallpapers has a project folder on disk.
    NoProject,
    /// The project folder has no preview image.
    NoPreview,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Failure::NotJson => "config.json is not valid JSON",
            Failure::NoWallpaperConfig => "config.json has no general.wallpaperconfig",
            Failure::NoSelection => "config.json selects no wallpaper",
            Failure::NoProject => "the selected wallpaper's folder is missing",
            Failure::NoPreview => "the wallpaper's folder has no preview image",
        })
    }
}

/// What `config.json` says is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The `?installdirectory` key, where relative paths start.
    pub install_dir: Option<PathBuf>,
    /// The selected wallpaper files for the primary monitor, best guess first.
    pub files: Vec<PathBuf>,
}

/// The monitor taken to be the primary one. Wallpaper Engine numbers monitors itself;
/// `Monitor0` is the first it lists.
const PRIMARY: &str = "Monitor0";

/// Reads the selected wallpapers out of `config.json`'s text.
///
/// Candidates, in order: the entry of a group (`group_...`) that contains the primary
/// monitor (a monitor in a group is driven by the group; its own entry can be a stale
/// leftover), then the primary monitor's own entry, then any other entry.
pub fn parse_config(text: &str) -> Result<Selection, Failure> {
    let root: Value = serde_json::from_str(text).map_err(|_| Failure::NotJson)?;
    let root = root.as_object().ok_or(Failure::NotJson)?;
    let install_dir = root
        .get("?installdirectory")
        .and_then(Value::as_str)
        .map(PathBuf::from);

    // The Steam user's name is the key; take the first user that has a wallpaper config.
    let config = root
        .iter()
        .filter(|(key, _)| !key.starts_with('?'))
        .find_map(|(_, user)| user.pointer("/general/wallpaperconfig"))
        .ok_or(Failure::NoWallpaperConfig)?;
    let selected = config
        .get("selectedwallpapers")
        .and_then(Value::as_object)
        .ok_or(Failure::NoWallpaperConfig)?;

    let file_of = |key: &str| {
        selected
            .get(key)
            .and_then(|entry| entry.get("file"))
            .and_then(Value::as_str)
            .filter(|file| !file.is_empty())
            .map(PathBuf::from)
    };

    let mut files: Vec<PathBuf> = Vec::new();
    let mut add = |file: Option<PathBuf>| {
        if let Some(file) = file
            && !files.contains(&file)
        {
            files.push(file);
        }
    };
    if let Some(groups) = config.pointer("/profile/groups").and_then(Value::as_object) {
        for (name, group) in groups {
            let has_primary = group
                .get("monitors")
                .and_then(Value::as_array)
                .is_some_and(|m| m.iter().any(|m| m.as_str() == Some(PRIMARY)));
            if has_primary {
                add(file_of(name));
            }
        }
    }
    add(file_of(PRIMARY));
    for key in selected.keys() {
        add(file_of(key));
    }

    if files.is_empty() {
        return Err(Failure::NoSelection);
    }
    Ok(Selection { install_dir, files })
}

/// The preview file named by a project's `project.json` text, relative to the project
/// folder. `None` if there is no usable `preview` key.
pub fn preview_name(project_json: &str) -> Option<PathBuf> {
    let value: Value = serde_json::from_str(project_json).ok()?;
    let name = PathBuf::from(value.get("preview")?.as_str()?);
    // Only a plain relative path: nothing that climbs out of the project folder.
    let plain = name
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    (plain && name.components().next().is_some()).then_some(name)
}

/// What a project folder holds when `project.json` doesn't name a preview.
const GUESSES: [&str; 5] = [
    "preview.jpg",
    "preview.png",
    "preview.gif",
    "preview.jpeg",
    "preview.webp",
];

/// The project folder of a selected wallpaper file: the file's own folder (or the
/// folder itself). A relative path starts at the install folder.
pub fn project_dir(file: &Path, install_dir: Option<&Path>) -> PathBuf {
    let full = if file.is_relative() {
        install_dir.map_or_else(|| file.to_path_buf(), |dir| dir.join(file))
    } else {
        file.to_path_buf()
    };
    if full.is_dir() {
        full
    } else {
        full.parent().map(Path::to_path_buf).unwrap_or(full)
    }
}

/// The preview image of the first selected wallpaper whose folder exists.
pub fn preview_image(selection: &Selection) -> Result<PathBuf, Failure> {
    let mut failure = Failure::NoProject;
    for file in &selection.files {
        let dir = project_dir(file, selection.install_dir.as_deref());
        if !dir.is_dir() {
            continue;
        }
        failure = Failure::NoPreview;
        let named = std::fs::read_to_string(dir.join("project.json"))
            .ok()
            .and_then(|text| preview_name(&text))
            .map(|name| dir.join(name));
        let found = named
            .into_iter()
            .chain(GUESSES.iter().map(|guess| dir.join(guess)));
        for path in found {
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wallpaper Engine 2.8.42, trimmed from a real file: a monitor group is selected,
    /// and `Monitor0` still has an old entry. `wallpaperconfigrecent` is a sibling that
    /// must be ignored.
    const REAL: &str = r#"{
      "?installdirectory": "C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine",
      "HP": { "general": {
        "browser": { "lastselectedmonitor": "group_-708543123" },
        "wallpaperconfig": {
          "layout": 0,
          "profile": { "groups": { "group_-708543123": { "layout": 2, "monitors": ["Monitor1", "Monitor0"] } } },
          "selectedwallpapers": {
            "Monitor0": { "file": "C:/Steam/steamapps/workshop/content/431960/111/scene.pkg" },
            "group_-708543123": { "file": "C:/Steam/steamapps/workshop/content/431960/222/scene.pkg" }
          }
        },
        "wallpaperconfigrecent": [ { "config": { "selectedwallpapers": { "Monitor0": { "file": "C:/old/999/scene.pkg" } } } } ]
      }, "version": "2.8.42", "wproperties": {} }
    }"#;

    #[test]
    fn a_group_with_the_primary_monitor_comes_before_its_own_entry() {
        let selection = parse_config(REAL).unwrap();
        assert_eq!(
            selection.files,
            [
                PathBuf::from("C:/Steam/steamapps/workshop/content/431960/222/scene.pkg"),
                PathBuf::from("C:/Steam/steamapps/workshop/content/431960/111/scene.pkg"),
            ]
        );
        assert_eq!(
            selection.install_dir,
            Some(PathBuf::from(
                "C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine"
            ))
        );
    }

    #[test]
    fn a_plain_single_monitor_config() {
        let text = r#"{ "Me": { "general": { "wallpaperconfig": { "selectedwallpapers": {
            "Monitor0": { "file": "D:\\Wallpapers\\cool\\project.json" } } } } } }"#;
        let selection = parse_config(text).unwrap();
        assert_eq!(
            selection.files,
            [PathBuf::from("D:\\Wallpapers\\cool\\project.json")]
        );
        assert_eq!(selection.install_dir, None);
    }

    #[test]
    fn the_primary_monitors_entry_comes_before_other_monitors() {
        let text = r#"{ "Me": { "general": { "wallpaperconfig": { "selectedwallpapers": {
            "Monitor1": { "file": "C:/b/scene.pkg" }, "Monitor0": { "file": "C:/a/scene.pkg" } } } } } }"#;
        let selection = parse_config(text).unwrap();
        assert_eq!(selection.files[0], PathBuf::from("C:/a/scene.pkg"));
        assert_eq!(selection.files.len(), 2);
    }

    #[test]
    fn broken_or_unexpected_files_fail_with_a_reason() {
        assert_eq!(parse_config("{ nope"), Err(Failure::NotJson));
        assert_eq!(parse_config("[1, 2]"), Err(Failure::NotJson));
        assert_eq!(parse_config("{}"), Err(Failure::NoWallpaperConfig));
        assert_eq!(
            parse_config(r#"{ "Me": { "general": { "browser": {} } } }"#),
            Err(Failure::NoWallpaperConfig)
        );
        assert_eq!(
            parse_config(
                r#"{ "Me": { "general": { "wallpaperconfig": { "selectedwallpapers": { "Monitor0": { "file": "" }, "Monitor1": {} } } } } }"#
            ),
            Err(Failure::NoSelection)
        );
    }

    #[test]
    fn the_preview_comes_from_project_json() {
        assert_eq!(
            preview_name(r#"{ "title": "x", "preview": "preview.gif" }"#),
            Some(PathBuf::from("preview.gif"))
        );
        assert_eq!(preview_name(r#"{ "title": "x" }"#), None);
        assert_eq!(preview_name("not json"), None);
        assert_eq!(preview_name(r#"{ "preview": "" }"#), None);
    }

    #[test]
    fn a_preview_cannot_climb_out_of_the_project_folder() {
        assert_eq!(preview_name(r#"{ "preview": "../secret.png" }"#), None);
        assert_eq!(preview_name(r#"{ "preview": "C:/secret.png" }"#), None);
        assert_eq!(preview_name(r#"{ "preview": "/secret.png" }"#), None);
        assert_eq!(
            preview_name(r#"{ "preview": "sub/p.png" }"#),
            Some(PathBuf::from("sub/p.png"))
        );
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qapsule-we-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_preview_image_is_found_in_the_project_folder() {
        let root = temp("found");
        let stale = root.join("stale"); // never created
        let live = root.join("live");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(live.join("project.json"), r#"{ "preview": "pic.png" }"#).unwrap();
        std::fs::write(live.join("pic.png"), b"x").unwrap();
        let selection = Selection {
            install_dir: None,
            files: vec![stale.join("scene.pkg"), live.join("scene.pkg")],
        };
        // The first project is missing: the next one is used.
        assert_eq!(preview_image(&selection), Ok(live.join("pic.png")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn without_a_named_preview_the_usual_names_are_tried() {
        let root = temp("guess");
        std::fs::write(root.join("preview.gif"), b"x").unwrap();
        let selection = Selection {
            install_dir: None,
            files: vec![root.join("video.mp4")],
        };
        assert_eq!(preview_image(&selection), Ok(root.join("preview.gif")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_folders_and_previews_are_told_apart() {
        let root = temp("none");
        let missing = Selection {
            install_dir: None,
            files: vec![root.join("nowhere").join("scene.pkg")],
        };
        assert_eq!(preview_image(&missing), Err(Failure::NoProject));
        let empty = Selection {
            install_dir: None,
            files: vec![root.join("scene.pkg")],
        };
        assert_eq!(preview_image(&empty), Err(Failure::NoPreview));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn relative_files_start_at_the_install_folder() {
        let root = temp("relative");
        let project = root.join("projects").join("defaultprojects").join("beach");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("preview.jpg"), b"x").unwrap();
        let selection = Selection {
            install_dir: Some(root.clone()),
            files: vec![PathBuf::from("projects/defaultprojects/beach/scene.json")],
        };
        assert_eq!(preview_image(&selection), Ok(project.join("preview.jpg")));
        let _ = std::fs::remove_dir_all(root);
    }
}
