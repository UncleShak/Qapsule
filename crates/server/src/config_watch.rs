//! Watches the config file's folder and reloads the file when it changes.
//!
//! Event-driven: the OS tells us about changes, a single timer is reset by each
//! event (a save is often several events), and nothing runs in between.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use qapsule_core::Settings;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::config;

/// How long the file must be quiet before it is reloaded.
pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// Keeps the watch alive. Dropping it stops the watcher and the reload task.
pub struct ConfigWatch {
    _watcher: RecommendedWatcher,
    task: JoinHandle<()>,
}

impl Drop for ConfigWatch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Watches `path`'s folder (created if missing) and calls `on_reload` with the
/// settings and warnings after each change that leaves a readable file. A file
/// that can't be read or parsed logs a short reason and calls nothing.
/// Call it from inside the tokio runtime.
pub fn watch(
    path: &Path,
    on_reload: impl Fn(Settings, Vec<String>) + Send + 'static,
) -> Result<ConfigWatch, &'static str> {
    let dir = path.parent().ok_or("the config path has no folder")?;
    let name = path.file_name().ok_or("the config path has no name")?;
    std::fs::create_dir_all(dir).map_err(|_| "couldn't create the config folder")?;

    let (tx, rx) = mpsc::unbounded_channel();
    let wanted = name.to_owned();
    let complained = AtomicBool::new(false);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        if event.is_err() && !complained.swap(true, Ordering::Relaxed) {
            eprintln!("qapsule: the config watcher reported an error; reloading");
        }
        if should_reload(&event, &wanted) {
            let _ = tx.send(());
        }
    })
    .map_err(|_| "couldn't start the file watcher")?;
    watcher
        .watch(dir, RecursiveMode::NonRecursive)
        .map_err(|_| "couldn't watch the config folder")?;

    let path = path.to_owned();
    let task = tokio::spawn(debounce(rx, DEBOUNCE, move || {
        match config::reload(&path) {
            Ok((settings, warnings)) => on_reload(settings, warnings),
            Err(reason) => eprintln!("qapsule: config not reloaded ({reason})"),
        };
    }));
    Ok(ConfigWatch {
        _watcher: watcher,
        task,
    })
}

/// Whether the event means the file called `name` may have changed. A watcher
/// error, a rescan request (events were lost) or an event naming no path could
/// hide a change, so each of those reloads too. Reads and other files do not.
fn should_reload(event: &notify::Result<Event>, name: &OsStr) -> bool {
    let Ok(event) = event else { return true };
    if event.need_rescan() || event.paths.is_empty() {
        return true;
    }
    !matches!(event.kind, EventKind::Access(_))
        && event.paths.iter().any(|p| p.file_name() == Some(name))
}

/// Calls `fire` once `delay` after the last event of a burst, using one timer
/// that each event pushes back. Ends when the senders are gone.
async fn debounce(mut rx: mpsc::UnboundedReceiver<()>, delay: Duration, mut fire: impl FnMut()) {
    while rx.recv().await.is_some() {
        let timer = tokio::time::sleep(delay);
        tokio::pin!(timer);
        loop {
            tokio::select! {
                event = rx.recv() => {
                    if event.is_none() {
                        return;
                    }
                    timer.as_mut().reset(Instant::now() + delay);
                }
                () = &mut timer => break,
            }
        }
        fire();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, ModifyKind};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    fn event(kind: EventKind, path: &str) -> Event {
        Event::new(kind).add_path(PathBuf::from(path))
    }

    #[test]
    fn only_changes_to_the_config_file_count() {
        let name = OsStr::new("config.toml");
        let modify = EventKind::Modify(ModifyKind::Any);
        let check = |e: Event| should_reload(&Ok(e), name);
        assert!(check(event(modify, "C:/x/config.toml")));
        assert!(check(event(
            EventKind::Create(CreateKind::File),
            "C:/x/config.toml"
        )));
        assert!(!check(event(modify, "C:/x/other.toml")));
        assert!(!check(event(modify, "C:/x/config.toml.tmp")));
        assert!(!check(event(
            EventKind::Access(AccessKind::Any),
            "C:/x/config.toml"
        )));
    }

    #[test]
    fn errors_rescans_and_pathless_events_reload_too() {
        let name = OsStr::new("config.toml");
        assert!(should_reload(&Err(notify::Error::generic("x")), name));
        let rescan =
            event(EventKind::Other, "C:/x/other.toml").set_flag(notify::event::Flag::Rescan);
        assert!(should_reload(&Ok(rescan), name));
        assert!(should_reload(
            &Ok(Event::new(EventKind::Modify(ModifyKind::Any))),
            name
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn a_burst_fires_once_after_the_quiet_period() {
        let (tx, rx) = mpsc::unbounded_channel();
        let fired = Arc::new(AtomicUsize::new(0));
        let count = fired.clone();
        let task = tokio::spawn(debounce(rx, DEBOUNCE, move || {
            count.fetch_add(1, Ordering::SeqCst);
        }));

        for _ in 0..3 {
            tx.send(()).unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        assert_eq!(
            fired.load(Ordering::SeqCst),
            0,
            "each event reset the timer"
        );
        tokio::time::sleep(Duration::from_millis(101)).await;
        assert_eq!(fired.load(Ordering::SeqCst), 1);

        tokio::time::sleep(Duration::from_secs(60)).await;
        assert_eq!(fired.load(Ordering::SeqCst), 1, "nothing runs on its own");

        tx.send(()).unwrap();
        tokio::time::sleep(DEBOUNCE + Duration::from_millis(1)).await;
        assert_eq!(fired.load(Ordering::SeqCst), 2);

        drop(tx);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn editing_the_real_file_reloads_it_and_a_broken_one_is_ignored() {
        let dir = std::env::temp_dir().join(format!("qapsule-watch-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);

        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let _watch = watch(&path, move |settings, _| {
            sink.lock().unwrap().push(settings.do_not_disturb);
        })
        .unwrap();

        std::fs::write(&path, "[notifications]\ndo_not_disturb = true\n").unwrap();
        wait_for(&seen, 1).await;
        std::fs::write(&path, "[notifications\n").unwrap();
        tokio::time::sleep(DEBOUNCE * 3).await;
        std::fs::write(&path, "[notifications]\ndo_not_disturb = false\n").unwrap();
        wait_for(&seen, 2).await;
        // The app's own atomic save (temp file, then rename) is noticed too.
        config::save_do_not_disturb(&path, true).unwrap();
        wait_for(&seen, 3).await;
        assert_eq!(*seen.lock().unwrap(), vec![true, false, true]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    async fn wait_for(seen: &Mutex<Vec<bool>>, len: usize) {
        for _ in 0..100 {
            if seen.lock().unwrap().len() >= len {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the reload never came");
    }
}
