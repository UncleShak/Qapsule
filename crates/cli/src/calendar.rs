//! `qapsule calendar`: keeps the secret iCal link in Windows Credential Manager.
//! It never goes through the pipe, and the link is never printed.

use std::io::{BufRead, IsTerminal, Write};

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum CalendarCommand {
    /// Store the secret iCal link, read from one line of stdin
    SetLink,
    /// Remove the stored link
    ClearLink,
    /// Say whether a link is set
    Status,
}

/// Windows Credential Manager refuses longer secrets.
const MAX_LINK: usize = 1000;

/// Checks a pasted link: `https://` is kept, `webcal://` becomes `https://`,
/// anything else is refused. The message never repeats the link.
pub fn validate_link(input: &str) -> Result<String, String> {
    let link = input.trim();
    if link.is_empty() {
        return Err("no link given. Paste the secret iCal address, then press Enter".into());
    }
    if link.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("that doesn't look like a link".into());
    }
    if link.len() > MAX_LINK {
        return Err("That link is too long for Windows Credential Manager.".into());
    }
    let lower = link.to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &link["https://".len()..]
    } else if lower.starts_with("webcal://") {
        &link["webcal://".len()..]
    } else {
        return Err("the link must start with https:// or webcal://".into());
    };
    if rest.is_empty() || rest.starts_with('/') {
        return Err("that doesn't look like a link".into());
    }
    Ok(format!("https://{rest}"))
}

pub fn run(command: CalendarCommand) -> Result<(), String> {
    let stdin = std::io::stdin();
    let prompt = stdin.is_terminal();
    execute(
        command,
        qapsule_secrets::CALENDAR_LINK,
        &mut stdin.lock(),
        prompt,
        &mut std::io::stdout(),
    )
}

/// The testable core of `run`: a target, an input and an output.
pub fn execute(
    command: CalendarCommand,
    target: &str,
    input: &mut impl BufRead,
    prompt: bool,
    out: &mut impl Write,
) -> Result<(), String> {
    const STORE_FAILED: &str = "couldn't reach Windows Credential Manager";
    let say = |out: &mut dyn Write, text: &str| writeln!(out, "{text}").map_err(|e| e.to_string());
    match command {
        CalendarCommand::SetLink => {
            if prompt {
                eprint!("Paste the secret iCal address and press Enter: ");
                let _ = std::io::stderr().flush();
            }
            let mut line = String::new();
            input
                .read_line(&mut line)
                .map_err(|_| "couldn't read the link".to_string())?;
            let link = validate_link(&line)?;
            qapsule_secrets::set(target, &link).map_err(|_| STORE_FAILED.to_string())?;
            say(out, "Calendar link saved.")
        }
        CalendarCommand::ClearLink => {
            let removed = qapsule_secrets::clear(target).map_err(|_| STORE_FAILED.to_string())?;
            say(
                out,
                if removed {
                    "Calendar link removed."
                } else {
                    "There was no calendar link to remove."
                },
            )
        }
        CalendarCommand::Status => {
            let set = qapsule_secrets::get(target)
                .map_err(|_| STORE_FAILED.to_string())?
                .is_some();
            say(
                out,
                if set {
                    "A calendar link is set."
                } else {
                    "No calendar link is set."
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_over_1000_characters_is_refused() {
        let at_limit = format!("https://example.com/{}", "a".repeat(1000 - 20));
        assert_eq!(at_limit.len(), 1000);
        assert!(validate_link(&at_limit).is_ok());
        let error = validate_link(&format!("{at_limit}a")).unwrap_err();
        assert_eq!(
            error,
            "That link is too long for Windows Credential Manager."
        );
    }

    #[test]
    fn https_links_are_kept_and_webcal_becomes_https() {
        assert_eq!(
            validate_link("  https://calendar.google.com/x/basic.ics \r\n").unwrap(),
            "https://calendar.google.com/x/basic.ics"
        );
        assert_eq!(
            validate_link("webcal://example.com/a.ics").unwrap(),
            "https://example.com/a.ics"
        );
        assert_eq!(
            validate_link("WEBCAL://example.com/a.ics").unwrap(),
            "https://example.com/a.ics"
        );
    }

    #[test]
    fn anything_else_is_refused_without_echoing_it() {
        for bad in [
            "",
            "   ",
            "http://example.com/a.ics",
            "file:///c:/secret.ics",
            "ftp://example.com/a",
            "example.com/a.ics",
            "https://",
            "https:///x",
            "https://exa mple.com",
        ] {
            let error = validate_link(bad).unwrap_err();
            assert!(bad.is_empty() || !error.contains(bad), "{error}");
        }
    }

    #[test]
    fn set_status_and_clear_use_the_given_target() {
        let target = format!("Qapsule/test-cli-{}", std::process::id());
        let run = |command, input: &str| {
            let mut out = Vec::new();
            let result = execute(command, &target, &mut input.as_bytes(), false, &mut out);
            (result, String::from_utf8(out).unwrap())
        };
        let (result, out) = run(CalendarCommand::Status, "");
        result.unwrap();
        assert_eq!(out, "No calendar link is set.\n");

        let (result, out) = run(CalendarCommand::SetLink, "http://nope\n");
        assert!(result.is_err() && out.is_empty());
        assert_eq!(qapsule_secrets::get(&target).unwrap(), None);

        let (result, out) = run(CalendarCommand::SetLink, "webcal://example.com/s.ics\n");
        result.unwrap();
        assert!(!out.contains("example.com"));
        assert_eq!(
            qapsule_secrets::get(&target).unwrap().as_deref(),
            Some("https://example.com/s.ics")
        );
        let (_, out) = run(CalendarCommand::Status, "");
        assert_eq!(out, "A calendar link is set.\n");

        let (_, out) = run(CalendarCommand::ClearLink, "");
        assert_eq!(out, "Calendar link removed.\n");
        let (_, out) = run(CalendarCommand::ClearLink, "");
        assert_eq!(out, "There was no calendar link to remove.\n");
    }
}
