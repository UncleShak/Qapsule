# Wallpaper Engine

How Qapsule's theme provider finds the current Wallpaper Engine wallpaper. Wallpaper Engine draws its own window behind the desktop icons, so `IDesktopWallpaper` still reports the old static wallpaper. The format is undocumented; this page records what was observed. The parser is `crates/providers/src/theme/wallpaper_engine.rs`, with tests.

**Verified** below means seen in a real install (Wallpaper Engine 2.8.42, Steam, checked 2026-11-01). **Unverified** means recalled from community knowledge and not seen; the parser tolerates it but nothing proves it.

## Where things are

- **Process (verified):** `wallpaper64.exe` or `wallpaper32.exe`, in the install folder. Qapsule asks Windows for the running process's image path, so it finds the install wherever the Steam library is.
- **Install folder (verified):** `<Steam library>\steamapps\common\wallpaper_engine`. Qapsule looks beside the running executable, one folder up (in case it runs from `bin`), then in `%ProgramFiles(x86)%\Steam\steamapps\common\wallpaper_engine`. It does not read `libraryfolders.vdf`; the process path makes that unnecessary.
- **`config.json` (verified):** in the install folder, about 24 KB, pretty-printed JSON. Wallpaper Engine keeps dated copies in `config_backups\config_YYYY-MM-DD.json`.
- **Workshop projects (verified):** `<Steam library>\steamapps\workshop\content\431960\<workshop id>\`.
- **Built-in and local projects (verified folders):** `<install>\projects\defaultprojects\<name>\` and `<install>\projects\myprojects\<name>\`.

## `config.json`

```json
{
  "?installdirectory": "C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine",
  "<steam user name>": {
    "general": {
      "wallpaperconfig": {
        "layout": 0,
        "profile": { "groups": { "group_-708543123": { "layout": 2, "monitors": ["Monitor1", "Monitor0"] } } },
        "selectedwallpapers": {
          "Monitor0":          { "file": "C:/.../431960/2229371846/scene.pkg" },
          "group_-708543123":  { "file": "C:/.../431960/1959836574/scene.pkg" }
        }
      },
      "wallpaperconfigrecent": [ ... ],
      "wallpaperconfigscreensaver": { ... }
    },
    "version": "2.8.42",
    "wproperties": { ... }
  }
}
```

All verified unless noted.

- The top level has `?installdirectory` and one key per Steam user (the account name). Qapsule takes the first user with `general.wallpaperconfig`.
- `general.wallpaperconfig.selectedwallpapers` maps a monitor (`Monitor0`, `Monitor1`, ...) or a monitor group (`group_<number>`) to `{ "file": "<path>" }`. Paths use forward slashes here; backslashes (unverified) work the same.
- `file` is the wallpaper's main file: `scene.pkg` for scenes, the video file for videos, `project.json` for some (unverified). The project folder is the file's folder. A relative path (unverified) starts at the install folder.
- `profile.groups.<name>.monitors` lists the monitors a group drives. In the observed file `Monitor0` was in a group and also had its own `selectedwallpapers` entry pointing at a project that no longer exists on disk: a stale leftover. The group's entry was the live one (`browser.lastselectedmonitor` was the group).
- `wallpaperconfigrecent` and `wallpaperconfigscreensaver` are siblings of `wallpaperconfig` holding history and the screen saver's choice. They are ignored.
- Playlists (unverified): an entry may carry playlist data instead of, or beside, `file`. Qapsule reads only `file`.
- Which `MonitorN` is the Windows primary monitor is not recorded. Qapsule assumes `Monitor0`, and prefers the entry of a group that contains it.

Qapsule's order of candidates: the group entry that contains `Monitor0`, then `Monitor0`'s own entry, then any other entry. The first whose project folder exists and has a preview wins.

## `project.json`

Verified keys: `title`, `type` (`scene`, `video`, `web`, `application`), `file`, `preview`, `tags`, `visibility`. `preview` is a file name in the project folder: `preview.gif` for workshop wallpapers, `preview.jpg` for the built-in ones. It can be an animated GIF; Qapsule decodes the first frame. If `preview` is missing, Qapsule tries `preview.jpg`, `.png`, `.gif`, `.jpeg`, `.webp`. A `preview` that is absolute or climbs out of the folder (`..`) is ignored.

The preview is a thumbnail of the wallpaper, not what is on screen: for a video or an interactive scene the live picture can differ. That is accepted for a palette.

## Watching

Qapsule watches the install folder (not the file: the file may be replaced when saved) with `notify`, only while Wallpaper Engine runs, and ignores events for any other file name. `log.txt` lives in the same folder and changes often; its events are filtered out in the callback. A change is settled for 400 ms before `config.json` is read again.

Wallpaper Engine starting or quitting is noticed only at the next event: the expanded view opening, a wallpaper change, an accent change or a settings change. This is on purpose: Windows raises no event for it and Qapsule does not poll, so the palette can be stale until then.

## Failure modes

Each logs one short line (once, until a different failure), then Qapsule falls through to the next source: the static wallpaper, one screen capture, then the Windows accent.

| Failure | Cause |
| --- | --- |
| `config.json wasn't found` | Wallpaper Engine runs from somewhere unexpected and the default Steam path has no copy. |
| `config.json can't be read` | The file is locked or being rewritten. The next change event retries. |
| `config.json is not valid JSON` | A write in progress, or a format change. |
| `has no general.wallpaperconfig` | A format change. |
| `selects no wallpaper` | No entry has a `file`. |
| `folder is missing` | The wallpaper was unsubscribed or moved; the entry is stale. |
| `folder has no preview image` | No `preview` key and no usual file name. |
| `the image can't be decoded` | An unsupported or damaged preview. |

The screen capture (the last source) is separate: it happens only at start-up, a wallpaper change or a settings change, never when the expanded view opens or while it is open, only when the desktop is what is on screen (the foreground window is the desktop or taskbar, or no visible window covers the primary monitor), and at most once per wallpaper state (path, modification time, length). Otherwise the Windows accent is used.

A wallpaper whose preview is grey is not a failure: the search ends and the Windows accent is used.
