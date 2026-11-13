//! Hooks: what the board starts by itself.
//!
//! Two kinds, each a prompt and the agent it is given to:
//!
//! - A **column hook** starts an agent on a card as it arrives in that column - dragged there by
//!   a person or moved there by an agent. One per column.
//! - A **schedule** starts one of the board's own agents at a time of day, every day or every
//!   weekday. As many as the board likes.
//!
//! Each is a file under `.moontasks/hooks/` - `columns/<column id>.md` and `board/<name>.md` -
//! with a few `key: value` lines between `---` fences and the prompt after them, so a hook is
//! edited from the board or in the file, by a person or by an agent, and both are the same
//! thing. No scripting: what a hook does is what its agent makes of the prompt.
//!
//! Every firing starts a new run, on the card or on the board task, marked with what started
//! it. Hooks run in the moon window last focused on the board, or else in whichever moon has
//! it open - a window or `moon serve` - and a claim on disk keeps two of them from firing the
//! same hook twice - see `runner`.

#[cfg(not(target_arch = "wasm32"))]
mod clock;
#[cfg(not(target_arch = "wasm32"))]
mod editing;
#[cfg(not(target_arch = "wasm32"))]
mod files;
#[cfg(not(target_arch = "wasm32"))]
mod firing;
#[cfg(not(target_arch = "wasm32"))]
mod runner;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
pub use editing::{list_hooks, remove_hook, save_hook};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use files::split_front_matter;
#[cfg(not(target_arch = "wasm32"))]
pub use runner::{note_window_focus, start_hook_runner};

use serde::{Deserialize, Serialize};

use crate::{api::AgentKind, moontasks::ColumnId};

/// How many runs hooks start on one card before they stop for it: a hook's agent moving the
/// card on into another hooked column is a chain, and two columns could otherwise hand a card
/// back and forth for ever. A person moving the card starts the count again - see
/// [`crate::moontasks::store::TaskMetadata::hook_runs_in_a_row`].
pub const HOOK_RUNS_IN_A_ROW_CAP: u32 = 5;

/// The folder under `.moontasks` the hooks are kept in.
pub const HOOKS_DIR_NAME: &str = "hooks";
/// Under [`HOOKS_DIR_NAME`], the folder of column hooks, one file per column.
pub const COLUMN_HOOKS_DIR_NAME: &str = "columns";
/// Under [`HOOKS_DIR_NAME`], the folder of the board's schedules, one file per schedule.
pub const SCHEDULES_DIR_NAME: &str = "board";
/// Under [`HOOKS_DIR_NAME`], the folder of review hooks, one file per review status.
pub const REVIEW_HOOKS_DIR_NAME: &str = "reviews";

/// The words a hook's prompt is filled with as it fires.
pub const COLUMN_HOOK_PLACEHOLDERS: &[&str] = &["{title}", "{notes}", "{from}", "{folder}", "{id}"];
/// The same for a review hook: the review that arrived, and the card it is on.
pub const REVIEW_HOOK_PLACEHOLDERS: &[&str] = &[
    "{title}", "{repo}", "{branch}", "{pr}", "{subject}", "{file}", "{from}", "{folder}", "{id}",
];
/// The same for a schedule, which is about no card.
pub const SCHEDULE_PLACEHOLDERS: &[&str] = &["{date}"];

/// What a column does when a card arrives in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnHook {
    pub column: ColumnId,
    /// The agent started on the card, or [`AgentKind::None`] for a shell the prompt is typed
    /// into as commands.
    pub start: AgentKind,
    pub prompt: String,
}

/// What a review status does when a review arrives in it - see [`crate::moontasks::reviews`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewHook {
    /// The status, which is the folder the review files are in.
    pub status: String,
    pub start: AgentKind,
    pub prompt: String,
}

/// How often a schedule runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleEvery {
    /// Every minute, whatever its time says.
    Minute,
    /// Every hour, at the minute its time says.
    Hour,
    Day,
    /// Monday to Friday.
    Weekdays,
}

impl ScheduleEvery {
    pub const ALL: [Self; 4] = [Self::Minute, Self::Hour, Self::Day, Self::Weekdays];

    /// How a schedule's file spells it, and how its editor offers it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Weekdays => "weekdays",
        }
    }

    /// Whether a time is asked for, and how much of one: none for every minute, the minute
    /// past for every hour, and the time of day otherwise.
    pub fn asks_for(self) -> TimeAsked {
        match self {
            Self::Minute => TimeAsked::Nothing,
            Self::Hour => TimeAsked::MinutePast,
            Self::Day | Self::Weekdays => TimeAsked::TimeOfDay,
        }
    }

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|days| days.word() == word)
    }
}

/// How much of a time a schedule runs at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeAsked {
    Nothing,
    /// `:30`, the minute past each hour.
    MinutePast,
    /// `08:30`.
    TimeOfDay,
}

/// A time of day, on the clock of the machine the board is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

impl TimeOfDay {
    /// `08:30`, the way a schedule's file and its pill write it.
    pub fn to_hh_mm(self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// `8:30` or `08:30`, and nothing else - not `8h30`, not `8:30pm`.
    pub fn parse_hh_mm(text: &str) -> Option<Self> {
        let (hour, minute) = text.trim().split_once(':')?;
        if minute.len() != 2 || hour.is_empty() || hour.len() > 2 {
            return None;
        }
        let hour: u8 = hour.parse().ok()?;
        let minute: u8 = minute.parse().ok()?;
        (hour < 24 && minute < 60).then_some(Self { hour, minute })
    }

    /// `:30`, `30` - the minute past the hour an hourly schedule runs at.
    pub fn parse_minute_past(text: &str) -> Option<Self> {
        let minute = text.trim().trim_start_matches(':');
        if minute.is_empty() || minute.len() > 2 {
            return None;
        }
        let minute: u8 = minute.parse().ok()?;
        (minute < 60).then_some(Self { hour: 0, minute })
    }

    /// The time as a schedule running `every` writes it: `08:30`, `:30`, or nothing.
    pub fn written_for(self, every: ScheduleEvery) -> String {
        match every.asks_for() {
            TimeAsked::Nothing => String::new(),
            TimeAsked::MinutePast => format!(":{:02}", self.minute),
            TimeAsked::TimeOfDay => self.to_hh_mm(),
        }
    }

    /// What a schedule running `every` makes of `text`, or `None` for text that is not a time
    /// of that kind.
    pub fn parse_for(every: ScheduleEvery, text: &str) -> Option<Self> {
        match every.asks_for() {
            TimeAsked::Nothing => Some(Self { hour: 0, minute: 0 }),
            TimeAsked::MinutePast => Self::parse_minute_past(text),
            TimeAsked::TimeOfDay => Self::parse_hh_mm(text),
        }
    }

    pub fn minute_of_day(self) -> u32 {
        u32::from(self.hour) * 60 + u32::from(self.minute)
    }
}

/// One of the board's own runs, started at a time of day.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    /// What it is called, which is also its file's name: lower case words joined by dashes.
    pub name: String,
    pub every: ScheduleEvery,
    pub at: TimeOfDay,
    pub start: AgentKind,
    pub prompt: String,
}

impl Schedule {
    /// `standup · 08:30`, `triage · weekdays 23:00`, `sync · hourly :15`, `ping · every
    /// minute` - what its pill on the board reads.
    pub fn pill_text(&self) -> String {
        let when = match self.every {
            ScheduleEvery::Minute => "every minute".to_string(),
            ScheduleEvery::Hour => format!("hourly :{:02}", self.at.minute),
            ScheduleEvery::Day => self.at.to_hh_mm(),
            ScheduleEvery::Weekdays => format!("weekdays {}", self.at.to_hh_mm()),
        };
        format!("{} · {when}", self.name)
    }
}

/// Every hook a board has, as the board draws them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardHooks {
    pub columns: Vec<ColumnHook>,
    /// In the order of their names.
    pub schedules: Vec<Schedule>,
    /// One per review status that has one, by status.
    #[serde(default)]
    pub reviews: Vec<ReviewHook>,
    /// A line for each hook file that could not be read, which is a hook that does not fire:
    /// the board says so rather than leaving someone to wonder why.
    #[serde(default)]
    pub unreadable: Vec<String>,
}

impl BoardHooks {
    pub fn of_column(&self, column: &ColumnId) -> Option<&ColumnHook> {
        self.columns.iter().find(|hook| hook.column == *column)
    }

    pub fn of_review_status(&self, status: &str) -> Option<&ReviewHook> {
        self.reviews.iter().find(|hook| hook.status == status)
    }
}

/// A hook written from the board.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SaveHookRequest {
    Column {
        hook: ColumnHook,
    },
    /// `renamed_from` is the name a schedule had before this save, when it was renamed in it:
    /// its old file is taken away.
    Schedule {
        schedule: Schedule,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        renamed_from: Option<String>,
    },
    Review {
        hook: ReviewHook,
    },
}

/// A hook taken off the board.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RemoveHookRequest {
    Column { column: ColumnId },
    Schedule { name: String },
    Review { status: String },
}

/// The agents a hook can start, by the word its file names them with. `shell` is a shell the
/// prompt is typed into as commands.
pub const HOOK_STARTS: &[(&str, AgentKind)] = &[
    ("claude", AgentKind::Claude),
    ("codex", AgentKind::Codex),
    ("opencode", AgentKind::OpenCode),
    ("pi", AgentKind::Pi),
    ("minerve", AgentKind::Minerve),
    ("shell", AgentKind::None),
];

/// The word a hook's file and its pill name what it starts by.
pub fn start_word(start: AgentKind) -> &'static str {
    HOOK_STARTS
        .iter()
        .find(|(_, kind)| *kind == start)
        .map_or("shell", |(word, _)| word)
}

pub fn start_from_word(word: &str) -> Option<AgentKind> {
    HOOK_STARTS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, kind)| *kind)
}

/// The name a schedule typed as `Morning standup` is kept under, which is its file's name:
/// lower case words joined by dashes - `morning-standup`.
pub fn schedule_name_of(typed: &str) -> String {
    let mut name = String::new();
    let mut pending_dash = false;
    for character in typed.chars() {
        if character.is_alphanumeric() {
            if pending_dash && !name.is_empty() {
                name.push('-');
            }
            pending_dash = false;
            name.extend(character.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    name
}

/// Where a column's hook is kept, from the repo root - what the hook's editor shows.
pub fn column_hook_repo_path(column: &ColumnId) -> String {
    format!(".moontasks/{HOOKS_DIR_NAME}/{COLUMN_HOOKS_DIR_NAME}/{column}.md")
}

/// Where a review status's hook is kept, from the repo root.
pub fn review_hook_repo_path(status: &str) -> String {
    format!(".moontasks/{HOOKS_DIR_NAME}/{REVIEW_HOOKS_DIR_NAME}/{status}.md")
}

/// Where a schedule is kept, from the repo root.
pub fn schedule_repo_path(name: &str) -> String {
    format!(".moontasks/{HOOKS_DIR_NAME}/{SCHEDULES_DIR_NAME}/{name}.md")
}
