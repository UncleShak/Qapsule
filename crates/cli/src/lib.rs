//! The `qapsule` command-line client. It talks to the app only through the
//! named pipe, using the protocol crate.

pub mod calendar;
pub mod client;
pub mod hook;
mod wrap;

use std::io::{self, Read};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use qapsule_protocol::limits::{MAX_SUBTITLE_CHARS, MAX_TITLE_CHARS};
use qapsule_protocol::{
    Action, ActionKind, Activity, ClientMessage, Progress, ServerMessage, State,
};

use client::Client;

#[derive(Debug, Parser)]
#[command(
    name = "qapsule",
    version = concat!(env!("CARGO_PKG_VERSION"), " (protocol v1)"),
    about = "Push live status into Qapsule, the status pill at the top of your screen",
    after_help = concat!("Protocol v1. Docs: ", env!("CARGO_PKG_REPOSITORY"))
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add an activity, or update the one with the same source and id
    Push(PushArgs),
    /// Mark an activity done; it leaves the pill a few seconds later
    Done(EndArgs),
    /// Mark an activity failed; it leaves the pill a few seconds later
    Fail(EndArgs),
    /// Show the live activities
    List {
        /// Only this source
        #[arg(long)]
        source: Option<String>,
        /// Print the raw protocol reply
        #[arg(long)]
        json: bool,
    },
    /// Remove activities at once
    Clear {
        /// Every activity from this source
        #[arg(long, required_unless_present = "all")]
        source: Option<String>,
        /// Only this activity (needs --source)
        #[arg(long, requires = "source")]
        id: Option<String>,
        /// Every activity from every source
        #[arg(long, conflicts_with_all = ["source", "id"])]
        all: bool,
    },
    /// Run a command, showing it as running and then done or failed
    Wrap(WrapArgs),
    /// Manage the calendar link (stored in Windows Credential Manager)
    Calendar {
        #[command(subcommand)]
        command: calendar::CalendarCommand,
    },
    /// Turn an agent's hook event (JSON on stdin) into an activity. Always silent.
    Hook {
        #[arg(value_enum)]
        agent: Agent,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Agent {
    ClaudeCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StateArg {
    Running,
    NeedsInput,
    Succeeded,
    Failed,
    Info,
}

impl From<StateArg> for State {
    fn from(s: StateArg) -> State {
        match s {
            StateArg::Running => State::Running,
            StateArg::NeedsInput => State::NeedsInput,
            StateArg::Succeeded => State::Succeeded,
            StateArg::Failed => State::Failed,
            StateArg::Info => State::Info,
        }
    }
}

#[derive(Debug, Args)]
pub struct PushArgs {
    /// Unique within the source; pushing it again updates the activity
    #[arg(long)]
    pub id: String,
    /// Who is pushing, e.g. my-script
    #[arg(long, default_value = "cli")]
    pub source: String,
    /// Main line in the pill (up to 80 characters)
    #[arg(long)]
    pub title: String,
    /// Second line (up to 120 characters)
    #[arg(long)]
    pub subtitle: Option<String>,
    #[arg(long, value_enum, default_value_t = StateArg::Running)]
    pub state: StateArg,
    /// 0 to 1, a percentage like 40%, or "indeterminate"
    #[arg(long, value_parser = parse_progress)]
    pub progress: Option<Progress>,
    /// Built-in icon name, e.g. agent, build, download
    #[arg(long)]
    pub icon: Option<String>,
    /// Seconds until it is removed
    #[arg(long)]
    pub expires_in: Option<u64>,
    /// Add a button that brings forward this process's window (or its parent's)
    #[arg(long, value_name = "PID", conflicts_with = "focus_parent")]
    pub focus_pid: Option<u32>,
    /// Like --focus-pid, using the process that ran qapsule (usually your terminal)
    #[arg(long)]
    pub focus_parent: bool,
    #[arg(long, default_value = "Open terminal")]
    pub focus_label: String,
    /// Add a button that opens this http or https link
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long, default_value = "Open link")]
    pub url_label: String,
    /// Add a button that calls back this command (see --wait). Repeatable.
    #[arg(long, value_name = "ID[:LABEL]")]
    pub callback: Vec<String>,
    /// Add a Dismiss button
    #[arg(long)]
    pub dismiss: bool,
    /// Stay connected until a callback button is pressed, then print its id
    #[arg(long)]
    pub wait: bool,
}

#[derive(Debug, Args)]
pub struct EndArgs {
    #[arg(long)]
    pub id: String,
    #[arg(long, default_value = "cli")]
    pub source: String,
    /// Replace the title
    #[arg(long)]
    pub title: Option<String>,
    /// Replace the subtitle
    #[arg(long)]
    pub subtitle: Option<String>,
}

#[derive(Debug, Args)]
pub struct WrapArgs {
    /// Title in the pill; defaults to the command line
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long, default_value = "wrap")]
    pub source: String,
    /// Defaults to wrap-<process id>
    #[arg(long)]
    pub id: Option<String>,
    /// The command to run, after --
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

fn parse_progress(text: &str) -> Result<Progress, String> {
    if text.eq_ignore_ascii_case("indeterminate") {
        return Ok(Progress::INDETERMINATE);
    }
    let (number, scale) = match text.strip_suffix('%') {
        Some(n) => (n, 100.0),
        None => (text, 1.0),
    };
    let value: f64 = number
        .trim()
        .parse()
        .map_err(|_| format!("`{text}` isn't a number, a percentage or \"indeterminate\""))?;
    let fraction = value / scale;
    if (0.0..=1.0).contains(&fraction) {
        Ok(Progress::Fraction(fraction))
    } else {
        Err(format!("`{text}` is outside 0 to 1 (0% to 100%)"))
    }
}

/// Shortens text to `max` characters, ending with an ellipsis when cut.
pub fn fit(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// A button bringing forward `pid`'s window.
pub fn focus_action(pid: u32, label: &str) -> Action {
    Action {
        id: "focus".into(),
        label: label.into(),
        kind: ActionKind::FocusWindow { pid },
        primary: true,
    }
}

pub fn dismiss_action() -> Action {
    Action {
        id: "dismiss".into(),
        label: "Dismiss".into(),
        kind: ActionKind::Dismiss,
        primary: false,
    }
}

impl PushArgs {
    /// The activity this command pushes. `parent` is the pid of the process that ran qapsule.
    pub fn activity(&self, parent: Option<u32>) -> Activity {
        let mut actions = Vec::new();
        let focus = self
            .focus_pid
            .or(if self.focus_parent { parent } else { None });
        if let Some(pid) = focus {
            actions.push(focus_action(pid, &self.focus_label));
        }
        if let Some(url) = &self.url {
            actions.push(Action {
                id: "url".into(),
                label: self.url_label.clone(),
                kind: ActionKind::OpenUrl { url: url.clone() },
                primary: false,
            });
        }
        for callback in &self.callback {
            let (id, label) = callback.split_once(':').unwrap_or((callback, callback));
            actions.push(Action {
                id: id.into(),
                label: label.into(),
                kind: ActionKind::Callback,
                primary: false,
            });
        }
        if self.dismiss {
            actions.push(dismiss_action());
        }
        // The first button that isn't Dismiss is the primary one.
        if let Some(first) = actions.iter_mut().find(|a| a.kind != ActionKind::Dismiss) {
            first.primary = true;
        }
        Activity {
            id: self.id.clone(),
            source: self.source.clone(),
            title: self.title.clone(),
            subtitle: self.subtitle.clone(),
            state: self.state.into(),
            progress: self.progress,
            icon: self.icon.clone(),
            expires_in: self.expires_in,
            actions,
        }
    }
}

impl EndArgs {
    pub fn message(&self, state: State) -> ClientMessage {
        ClientMessage::End {
            source: self.source.clone(),
            id: self.id.clone(),
            state,
            title: self.title.as_deref().map(|t| fit(t, MAX_TITLE_CHARS)),
            subtitle: self.subtitle.as_deref().map(|t| fit(t, MAX_SUBTITLE_CHARS)),
        }
    }
}

/// The process that started this one.
pub fn parent_pid() -> Option<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    let me = std::process::id();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            if entry.th32ProcessID == me {
                found = Some(entry.th32ParentProcessID);
                break;
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

/// Sends one message and turns the reply into an exit code.
fn send_one(message: &ClientMessage) -> Result<(), String> {
    let mut client = Client::connect().map_err(|e| e.to_string())?;
    expect_ok(client.request(message).map_err(|e| e.to_string())?)
}

fn expect_ok(reply: ServerMessage) -> Result<(), String> {
    match reply {
        ServerMessage::Ok => Ok(()),
        ServerMessage::Error { message, .. } => Err(message),
        other => Err(format!("unexpected reply: {other:?}")),
    }
}

fn state_name(state: State) -> &'static str {
    match state {
        State::Running => "running",
        State::NeedsInput => "needs input",
        State::Succeeded => "done",
        State::Failed => "failed",
        State::Info => "info",
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Push(args) => {
            let activity = args.activity(parent_pid());
            activity.validate().map_err(|e| e.to_string())?;
            let (source, id) = (activity.source.clone(), activity.id.clone());
            let mut client = Client::connect().map_err(|e| e.to_string())?;
            expect_ok(
                client
                    .request(&ClientMessage::Upsert { activity })
                    .map_err(|e| e.to_string())?,
            )?;
            if args.wait {
                loop {
                    match client.receive().map_err(|e| e.to_string())? {
                        ServerMessage::Action {
                            source: s,
                            id: i,
                            action,
                        } if s == source && i == id => {
                            println!("{action}");
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
            Ok(())
        }
        Command::Calendar { command } => calendar::run(command),
        Command::Done(args) => send_one(&args.message(State::Succeeded)),
        Command::Fail(args) => send_one(&args.message(State::Failed)),
        Command::Clear { source, id, all } => {
            debug_assert!(all || source.is_some());
            send_one(&ClientMessage::Clear { source, id })
        }
        Command::List { source, json } => {
            let mut client = Client::connect().map_err(|e| e.to_string())?;
            let reply = client
                .request(&ClientMessage::List { source })
                .map_err(|e| e.to_string())?;
            let ServerMessage::ListResult { activities } = reply else {
                return expect_ok(reply);
            };
            if json {
                println!(
                    "{}",
                    qapsule_protocol::encode(&ServerMessage::ListResult { activities })
                );
            } else if activities.is_empty() {
                println!("Nothing happening.");
            } else {
                for a in activities {
                    let progress = match a.progress {
                        Some(Progress::Fraction(f)) => format!(" {:.0}%", f * 100.0),
                        _ => String::new(),
                    };
                    let subtitle = a.subtitle.map(|s| format!(" · {s}")).unwrap_or_default();
                    println!(
                        "{:<12} {}/{}  {}{subtitle}{progress}",
                        state_name(a.state),
                        a.source,
                        a.id,
                        a.title
                    );
                }
            }
            Ok(())
        }
        Command::Wrap(args) => {
            let code = wrap::run(&args);
            std::process::exit(code);
        }
        Command::Hook { agent } => {
            let mut input = String::new();
            let _ = io::stdin().read_to_string(&mut input);
            let message = match agent {
                Agent::ClaudeCode => hook::claude_code(&input, parent_pid()),
            };
            // Hooks must never get in the agent's way: no output, no failure.
            if let Some(message) = message
                && let Ok(mut client) = Client::connect()
            {
                let _ = client.request(&message);
            }
            Ok(())
        }
    }
}

pub fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("qapsule: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
