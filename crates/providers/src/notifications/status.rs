//! Whether live Windows notifications are working, as the tray shows it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    On,
    NeedsPermission,
    Unavailable,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Starting => "Windows notifications: starting…",
            Status::On => "Windows notifications: on",
            Status::NeedsPermission => "Windows notifications: allow in Settings…",
            Status::Unavailable => "Windows notifications: unavailable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(Status::Starting.label(), "Windows notifications: starting…");
        assert_eq!(Status::On.label(), "Windows notifications: on");
        assert_eq!(
            Status::NeedsPermission.label(),
            "Windows notifications: allow in Settings…"
        );
        assert_eq!(
            Status::Unavailable.label(),
            "Windows notifications: unavailable"
        );
    }
}
