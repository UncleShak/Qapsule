//! A player's name from its app user model id, when Windows has no display name for it.

/// Never the whole id, which can be an install path: `C:\...\vlc.exe` is `vlc`,
/// `Spotify.exe` is `Spotify`, and `Pkg_123!Vendor.App` is `App`.
pub fn from_aumid(aumid: &str) -> String {
    let aumid = aumid.trim();
    let last = match aumid.rsplit_once('!') {
        Some((_, app)) => app.rsplit('.').next().unwrap_or(app),
        None => aumid.rsplit(['\\', '/']).next().unwrap_or(aumid),
    };
    let name = strip_exe(last).trim();
    if name.is_empty() {
        "Player".into()
    } else {
        name.into()
    }
}

fn strip_exe(name: &str) -> &str {
    match name.len().checked_sub(4).and_then(|at| name.get(at..)) {
        Some(tail) if tail.eq_ignore_ascii_case(".exe") => &name[..name.len() - 4],
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_names_paths_and_packages_become_short_names() {
        assert_eq!(from_aumid("Spotify.exe"), "Spotify");
        assert_eq!(from_aumid("chrome.EXE"), "chrome");
        assert_eq!(from_aumid(r"C:\Program Files\VideoLAN\VLC\vlc.exe"), "vlc");
        assert_eq!(from_aumid("/usr/bin/player.exe"), "player");
        assert_eq!(
            from_aumid("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"),
            "ZuneMusic"
        );
        assert_eq!(from_aumid("MSEdge"), "MSEdge");
        assert_eq!(from_aumid(""), "Player");
        assert_eq!(from_aumid(".exe"), "Player");
        assert_eq!(from_aumid(r"C:\dir\"), "Player");
    }
}
