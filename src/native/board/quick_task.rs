//! Quick task - a card made from the palette's query, with an agent already working on it.
//!
//! What is typed into the palette is offered as the title of a new card, on one row with a
//! chip per agent that can be started. Enter starts the agent picked, a chip its own. That
//! makes the card at the top of the board's first column, starts the agent on it - which
//! moves the card to in progress, the one start that does - and tells the
//! agent the title as its first line, so it starts on the work rather than waiting to be
//! told it. Nothing opens: the palette was
//! opened over something else, and that is what stays in front. The card's agent dot says it
//! started; only a failure gets a message.

use anyhow::Context;

use crate::{
    api::AgentKind,
    harnesses::HarnessStart,
    moontasks::{
        ColumnEnd, CreateTaskRequest, StartFolder, StartResourceRequest, TaskResourceKind,
    },
    native::{
        app::App,
        board::{agent_label, available_agents, harnesses_of},
        palette::{Command, CommandAction},
    },
    palette_history::{HistoryEntry, HistoryRecord, HistoryThing},
};

/// What a quick task is started with: one of moon's agents, or a harness of the person's -
/// see [`crate::harnesses`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum QuickStart {
    Agent(AgentKind),
    Harness(String),
}

impl QuickStart {
    /// What its chip and the row's title call it.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Agent(agent) => agent_label(*agent),
            Self::Harness(name) => name.clone(),
        }
    }

    /// The two a history entry keeps it as - see [`HistoryThing::QuickTask`].
    pub(crate) fn of_history(agent: AgentKind, harness: Option<&str>) -> Self {
        match harness {
            Some(name) => Self::Harness(name.to_string()),
            None => Self::Agent(agent),
        }
    }
}

/// The quick task the palette offers for what is typed: its title, what can be started on it,
/// and the one Enter starts.
pub(crate) struct QuickTask {
    pub(crate) title: String,
    /// The agents in the order the server lists them, then the person's harnesses in the
    /// order their settings do - which the row's chips keep whichever is picked.
    pub(crate) starts: Vec<QuickStart>,
    pub(crate) start: QuickStart,
}

/// The quick task for `query` as a title, with the agent the last quick task was started
/// with, or else the window's selected agent, or else the first of them. None for an empty
/// query, which is no title, or a machine with neither an agent nor a harness to start.
pub(crate) fn offered(app: &App, query: &str) -> Option<QuickTask> {
    let title = query.trim();
    if title.is_empty() {
        return None;
    }
    let selected = app
        .model
        .review_ref(&app.model.root_session_id)
        .and_then(|review| review.payload.as_ref())
        .map(|payload| payload.selected_agent);
    let last_started = app
        .model
        .settings
        .as_ref()
        .map(|settings| settings.quick_task_agent);
    let agents: Vec<AgentKind> = available_agents(app)
        .into_iter()
        .filter(|agent| *agent != AgentKind::None)
        .collect();
    let remembered = [last_started, selected]
        .into_iter()
        .flatten()
        .find(|agent| agents.contains(agent))
        .map(QuickStart::Agent);
    let starts: Vec<QuickStart> = agents
        .into_iter()
        .map(QuickStart::Agent)
        .chain(
            harnesses_of(app)
                .iter()
                .map(|harness| QuickStart::Harness(harness.name.clone())),
        )
        .collect();
    let start = remembered.or_else(|| starts.first().cloned())?;
    Some(QuickTask {
        title: title.to_string(),
        starts,
        start,
    })
}

impl QuickTask {
    /// The palette's row for it: the title, and what it starts with.
    pub(crate) fn command(&self) -> Command {
        Command {
            title: format!(
                "\"{}\" → start with {}",
                cut_title(&self.title),
                self.start.label()
            ),
            description: String::new(),
            action: self.started_with(self.start.clone()),
            shortcut: None,
        }
    }

    /// Where in the row's title the name of what it starts with is, to write it in its colour.
    pub(crate) fn start_in_title(&self, title: &str) -> std::ops::Range<usize> {
        let label = self.start.label();
        assert!(
            title.ends_with(&label),
            "a quick task's title ends with what it starts"
        );
        title.len() - label.len()..title.len()
    }

    /// What starting it with `start` does.
    pub(crate) fn started_with(&self, start: QuickStart) -> CommandAction {
        CommandAction::StartQuickTask {
            title: self.title.clone(),
            start,
        }
    }
}

/// How much of the typed title a row shows. The card and the agent get the whole of it.
const ROW_TITLE_CHARS: usize = 20;

/// The typed title as a row shows it: its first [`ROW_TITLE_CHARS`], and an ellipsis where
/// the rest was.
fn cut_title(title: &str) -> String {
    if title.chars().count() <= ROW_TITLE_CHARS {
        return title.to_string();
    }
    let kept: String = title.chars().take(ROW_TITLE_CHARS).collect();
    format!("{kept}…")
}

/// Make the card, start `start` on it and tell it `title`, and keep it in the palette's history
/// by the card - see [`crate::native::palette::history`]. A harness is handed the title as its
/// prompt as it starts, rather than told it after.
pub(crate) fn start(app: &mut App, title: String, start: QuickStart) {
    let (agent, harness) = match &start {
        QuickStart::Agent(agent) => {
            // Only an agent is offered again by the next quick task.
            app.remember_quick_task_agent(*agent);
            (*agent, None)
        }
        QuickStart::Harness(name) => (AgentKind::None, Some(name.clone())),
    };
    let session_id = app.model.root_session_id.clone();
    let gone = crate::native::palette::history::gone_things(app);
    // The filter goes, so the new card is on the board rather than behind a query that was
    // asked before it existed.
    app.model.board.filter.clear();
    app.tasks.spawn(
        move |backend| {
            let column = backend
                .list_columns(&session_id)?
                .into_iter()
                .next()
                .context("the board has no columns to put a card in")?
                .id;
            let task = backend.create_task(
                &session_id,
                &CreateTaskRequest {
                    title: title.clone(),
                    status: column,
                    joins: ColumnEnd::Top,
                },
            )?;
            let terminal_id = backend.start_task_resource(
                &session_id,
                &task.id,
                StartResourceRequest {
                    kind: TaskResourceKind::Agent,
                    agent,
                    opens_in: StartFolder::Repo,
                    unattended: false,
                    moves_to_in_progress: true,
                    harness: harness.as_ref().map(|name| HarnessStart {
                        name: name.clone(),
                        prompt: title.clone(),
                    }),
                },
            )?;
            // Waits in the shell until the agent has a box to take it - see `told` in
            // `crate::terminal`.
            if harness.is_none() {
                backend.tell_terminal(&terminal_id, &title)?;
            }
            let record = HistoryRecord {
                entry: HistoryEntry {
                    thing: HistoryThing::QuickTask {
                        task_id: task.id,
                        title: title.clone(),
                        agent,
                        harness: harness.clone(),
                    },
                    ran_at_unix: crate::native::messages::now_unix(),
                },
                gone: gone.clone(),
            };
            // The task is started either way: a history that could not be written is said
            // apart from it.
            let kept = backend.record_in_palette_history(&session_id, &record);
            Ok((record, kept))
        },
        move |model, result| {
            model.board.refresh_requested = true;
            match result {
                Ok((record, kept)) => {
                    crate::native::palette::history::add_recorded(model, record);
                    model.report(
                        kept,
                        "could not keep the quick task in the palette's history",
                    );
                }
                Err(error) => model.error(format!("could not start the quick task: {error}")),
            }
        },
    );
}
