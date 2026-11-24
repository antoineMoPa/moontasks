//! Harnesses - the person's own things to start on a task: a name and the command it runs.
//!
//! The agents moon knows are a fixed list - see [`AgentKind`]. A harness is how a person
//! starts anything else, or one of those agents with flags of their own, for the kinds of
//! task that want it: `opus-plan` for `claude --model opus --permission-mode plan`, `aider`
//! for `aider --message-file {brief_file}`. They are kept in the person's settings - see
//! [`crate::settings::Settings::harnesses`] - and offered wherever an agent is: a card's
//! `[start]`, a hook's editor and file, and the palette's quick task.
//!
//! A command whose program is an agent moon knows is still that agent: it is started the way
//! moon starts it, with the command's own arguments after moon's, so its run is recorded on
//! the card, resumed and sent comments like any other. Any other command is a line of shell,
//! run by the person's login shell in the repo, and listed on the card while it runs.
//!
//! The command is filled in before it runs - see [`HARNESS_PLACEHOLDERS`]. A line of shell
//! gets each value quoted as one word, so a title with a space or a quote in it is still one
//! argument.

use serde::{Deserialize, Serialize};

use crate::{api::AgentKind, moontasks::hooks::HOOK_STARTS};

/// What a harness's command is filled in with: what it was started to do - a hook's prompt,
/// a quick task's title, nothing from a card's `[start]` - the card's title, the brief and
/// the file it is in, the task's folder, and the repo.
pub const HARNESS_PLACEHOLDERS: &[&str] = &[
    "{prompt}",
    "{title}",
    "{brief}",
    "{brief_file}",
    "{task}",
    "{repo}",
];

/// The placeholder that hands the command what it was started to do.
const PROMPT_PLACEHOLDER: &str = "{prompt}";

/// The environment variable a harness's command reads its prompt from, whether or not its
/// command line names `{prompt}`.
pub const PROMPT_ENV_VAR: &str = "MOONTASKS_PROMPT";

/// A thing of the person's own to start on a task.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Harness {
    /// What it is offered as, and what a hook's file names it by: lower case letters, digits
    /// and dashes, and none of the words a hook already starts something by.
    pub name: String,
    /// The command line, with [`HARNESS_PLACEHOLDERS`] in it.
    pub command: String,
}

/// Which harness a start asked for, and what it is started to do.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HarnessStart {
    pub name: String,
    /// A hook's prompt or a quick task's title. Empty from a card's `[start]`.
    #[serde(default)]
    pub prompt: String,
}

impl HarnessStart {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            prompt: String::new(),
        }
    }
}

/// What a harness is not allowed to be, in the words the settings pane says it with.
pub fn check_harness(harness: &Harness) -> Result<(), String> {
    check_harness_name(&harness.name)?;
    if harness.command.trim().is_empty() {
        return Err(format!("{} has no command to run", harness.name));
    }
    Ok(())
}

/// Whether `name` can name a harness: it is a word a hook's file can say after `start:`
/// without meaning something else.
pub fn check_harness_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("a harness needs a name".to_string());
    }
    let is_a_word = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if !is_a_word {
        return Err(format!(
            "{name:?} is not a name: lower case letters, digits and dashes"
        ));
    }
    if HOOK_STARTS.iter().any(|(word, _)| *word == name) {
        return Err(format!("{name} is already what moon calls one of its own"));
    }
    Ok(())
}

/// What a harness's command is filled in from, for one run.
#[derive(Clone, Default)]
pub struct HarnessFillings {
    pub prompt: String,
    pub title: String,
    pub brief: String,
    pub brief_file: String,
    pub task: String,
    pub repo: String,
}

impl HarnessFillings {
    fn value_of(&self, placeholder: &str) -> &str {
        match placeholder {
            "{prompt}" => &self.prompt,
            "{title}" => &self.title,
            "{brief}" => &self.brief,
            "{brief_file}" => &self.brief_file,
            "{task}" => &self.task,
            "{repo}" => &self.repo,
            _ => "",
        }
    }
}

/// What a harness turns out to run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HarnessProgram {
    /// One of moon's agents, with these arguments after the ones moon starts it with.
    Agent {
        agent: AgentKind,
        extra_args: Vec<String>,
    },
    /// A line for the person's login shell.
    ShellLine(String),
}

/// A harness's command, filled in and ready to start.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HarnessRun {
    pub program: HarnessProgram,
    /// Whether the command line took the prompt itself. An agent whose line did not is told
    /// it once it is up, as a hook's agent is.
    pub takes_prompt: bool,
}

/// Work out what `harness` runs, with its command filled in from `fillings`.
pub fn plan_harness_run(harness: &Harness, fillings: &HarnessFillings) -> HarnessRun {
    let takes_prompt = harness.command.contains(PROMPT_PLACEHOLDER);
    let program = match split_agent_command(&harness.command) {
        Some((agent, arguments)) => HarnessProgram::Agent {
            agent,
            extra_args: arguments
                .iter()
                .map(|argument| fill_command(argument, fillings, QuoteValues::No))
                .collect(),
        },
        None => HarnessProgram::ShellLine(fill_command(
            harness.command.trim(),
            fillings,
            QuoteValues::ForShell,
        )),
    };
    HarnessRun {
        program,
        takes_prompt,
    }
}

/// The agent a harness is one of moon's with flags, or `None` for a command of its own.
pub fn find_agent_of_harness(harness: &Harness) -> Option<AgentKind> {
    split_agent_command(&harness.command).map(|(agent, _)| agent)
}

/// The agent a command starts and the arguments it gives it, when its first word is the
/// program of one moon knows. A command that is not plain words - an unclosed quote, a
/// variable set ahead of the program - is a line of shell.
fn split_agent_command(command: &str) -> Option<(AgentKind, Vec<String>)> {
    let mut words = shlex::split(command)?.into_iter();
    let program = words.next()?;
    // An agent's program is the word a hook starts it by - see `crate::agent_programs`.
    let (_, agent) = HOOK_STARTS
        .iter()
        .find(|(word, agent)| *agent != AgentKind::None && *word == program)?;
    let agent = *agent;
    Some((agent, words.collect()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuoteValues {
    No,
    ForShell,
}

/// The command with each placeholder filled in, in one pass - so a title holding `{brief}` is
/// not filled in again.
fn fill_command(command: &str, fillings: &HarnessFillings, quote: QuoteValues) -> String {
    let mut filled = String::with_capacity(command.len());
    let mut rest = command;
    while let Some(open) = rest.find('{') {
        filled.push_str(&rest[..open]);
        let after = &rest[open..];
        match HARNESS_PLACEHOLDERS
            .iter()
            .find(|placeholder| after.starts_with(**placeholder))
        {
            Some(placeholder) => {
                let value = fillings.value_of(placeholder);
                match quote {
                    QuoteValues::No => filled.push_str(value),
                    QuoteValues::ForShell => {
                        filled.push_str(&crate::shell_quoting::single_quoted(value))
                    }
                }
                rest = &after[placeholder.len()..];
            }
            None => {
                filled.push('{');
                rest = &after[1..];
            }
        }
    }
    filled.push_str(rest);
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harness(command: &str) -> Harness {
        Harness {
            name: "mine".to_string(),
            command: command.to_string(),
        }
    }

    fn fillings() -> HarnessFillings {
        HarnessFillings {
            prompt: "Fix Ada's bug".to_string(),
            title: "the {brief} card".to_string(),
            brief: "BRIEF".to_string(),
            brief_file: "/board/task/brief.md".to_string(),
            task: "/board/task".to_string(),
            repo: "/repo".to_string(),
        }
    }

    #[test]
    fn a_command_of_one_of_moons_agents_is_that_agent_with_its_arguments() {
        let run = plan_harness_run(
            &harness("claude --model opus --append-system-prompt 'be brief' {title}"),
            &fillings(),
        );
        assert_eq!(
            run.program,
            HarnessProgram::Agent {
                agent: AgentKind::Claude,
                extra_args: vec![
                    "--model".to_string(),
                    "opus".to_string(),
                    "--append-system-prompt".to_string(),
                    "be brief".to_string(),
                    // Filled in once: the title's own braces are left as they are.
                    "the {brief} card".to_string(),
                ],
            }
        );
        assert!(!run.takes_prompt);
    }

    #[test]
    fn any_other_command_is_a_line_of_shell_with_its_values_quoted() {
        let run = plan_harness_run(
            &harness("~/bin/triage.sh {task} --say {prompt}"),
            &fillings(),
        );
        assert_eq!(
            run.program,
            HarnessProgram::ShellLine(
                r"~/bin/triage.sh '/board/task' --say 'Fix Ada'\''s bug'".to_string()
            )
        );
        assert!(run.takes_prompt);
    }

    #[test]
    fn a_variable_set_ahead_of_an_agent_makes_it_a_line_of_shell() {
        let run = plan_harness_run(&harness("DEBUG=1 claude"), &fillings());
        assert_eq!(
            run.program,
            HarnessProgram::ShellLine("DEBUG=1 claude".to_string())
        );
    }

    #[test]
    fn a_name_is_a_word_that_is_none_of_moons_own() {
        assert!(check_harness_name("opus-plan").is_ok());
        assert!(check_harness_name("").is_err());
        assert!(check_harness_name("Opus Plan").is_err());
        assert!(check_harness_name("claude").is_err());
        assert!(check_harness_name("shell").is_err());
        assert!(
            check_harness(&Harness {
                name: "aider".to_string(),
                command: "  ".to_string()
            })
            .is_err()
        );
    }
}
