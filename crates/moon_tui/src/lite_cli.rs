//! This module reads the command line of `moon` without a window: the board in this terminal,
//! and the commands the full `moon` answers without one - the cards, the wire, the agents and the
//! MCP server - with the same words, so what an agent is told to type works with either.

use std::path::PathBuf;

use anyhow::{Result, bail};
use moon_core::{
    PROGRAM,
    cli::{agent, mcp, tasks, wire},
};

/// Says what the command line asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum LiteCommand {
    Help,
    Version,
    /// Opens the board of the repo this folder is in - or of the one named - in this terminal.
    Tui {
        repo: Option<PathBuf>,
    },
    Tasks(tasks::TaskCommand),
    Wire(wire::WireCommand),
    Agent(agent::AgentCommand),
    Mcp(mcp::McpCommand),
}

/// This is the option that names the repo whose board is opened.
const REPO_OPTION: &str = "--repo";

/// Reads a command line, the words after `moon`.
pub fn parse_lite_command(args: &[String]) -> Result<LiteCommand> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(LiteCommand::Help);
    };
    let asks_for_help = rest.iter().any(|arg| arg == "--help" || arg == "-h");
    match command.as_str() {
        "--help" | "-h" | "help" => Ok(LiteCommand::Help),
        "--version" | "-v" => Ok(LiteCommand::Version),
        // `moon tasks` is the board, as it is in the full moon: here it is drawn in the
        // terminal. Its card commands are the same ones.
        "tui" | "tasks" => {
            if let Some((word, words)) = rest.split_first()
                && command == "tasks"
                && let Some(card_command) = tasks::card_command_named(word)
            {
                if asks_for_help {
                    return Ok(LiteCommand::Help);
                }
                return Ok(LiteCommand::Tasks(tasks::parse(card_command, words)?));
            }
            if asks_for_help {
                return Ok(LiteCommand::Help);
            }
            parse_tui_args(rest)
        }
        "wire" => Ok(LiteCommand::Wire(wire::parse(rest, asks_for_help)?)),
        "agent" => Ok(LiteCommand::Agent(agent::parse(rest, asks_for_help)?)),
        "mcp" => Ok(LiteCommand::Mcp(mcp::parse(rest, asks_for_help)?)),
        other => bail!("`{PROGRAM} {other}` is not a command\n\n{}", help_text()),
    }
}

/// Reads `moon tui [--repo <path>]`, or `moon tasks [<folder>]`: the repo the board is
/// opened on.
fn parse_tui_args(args: &[String]) -> Result<LiteCommand> {
    match args {
        [] => Ok(LiteCommand::Tui { repo: None }),
        [option, repo] if option == REPO_OPTION => Ok(LiteCommand::Tui {
            repo: Some(PathBuf::from(repo)),
        }),
        [folder] if !folder.starts_with('-') => Ok(LiteCommand::Tui {
            repo: Some(PathBuf::from(folder)),
        }),
        _ => bail!(
            "`{PROGRAM} tui` takes a folder, or `{REPO_OPTION} <path>`, and nothing else\n\n{}",
            help_text()
        ),
    }
}

/// Does what the command line asked.
pub fn run_lite_command(command: LiteCommand) -> Result<()> {
    match command {
        LiteCommand::Help => {
            println!("{}", help_text());
            Ok(())
        }
        LiteCommand::Version => {
            println!("{PROGRAM} {} (in a terminal)", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        LiteCommand::Tui { repo } => crate::run_the_tui(repo),
        LiteCommand::Tasks(command) => tasks::run(command),
        LiteCommand::Wire(command) => wire::run(command),
        LiteCommand::Agent(command) => agent::run(command),
        LiteCommand::Mcp(command) => mcp::run(command),
    }
}

/// Returns what `moon --help` prints.
pub fn help_text() -> String {
    format!(
        "{PROGRAM}

lunar dev tools, in a terminal.

Usage:
  {PROGRAM} tui [--repo <path>]       the task board, its agents and shells, in this terminal
  {PROGRAM} tasks [<folder>]          the same
  {PROGRAM} tasks new [--appears-on <yyyy-mm-dd>] <title>
  {PROGRAM} tasks list                show the board's cards
  {PROGRAM} tasks move <column>       move this shell's task to a column
  {PROGRAM} wire post <one line>      tell the board's other agents
  {PROGRAM} agent <command>           list, start, view or tell the board's agents
  {PROGRAM} mcp [--repo <path>]       serve the board to an agent, as an MCP server
  {PROGRAM} --version
  {PROGRAM} --help

Examples:
  {PROGRAM} tui
  {PROGRAM} tasks new \"fix the races\"
  {PROGRAM} wire post \"rewriting src/cli\"
  {PROGRAM} agent tell fix-the-races \"start with the tests\"

This {PROGRAM} has no window: the full one also reviews, edits and opens windows."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(words: &[&str]) -> Result<LiteCommand> {
        parse_lite_command(
            &words
                .iter()
                .map(|word| word.to_string())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn the_board_is_opened_in_the_terminal_by_tui_or_tasks() {
        assert_eq!(parse(&["tui"]).unwrap(), LiteCommand::Tui { repo: None });
        assert_eq!(parse(&["tasks"]).unwrap(), LiteCommand::Tui { repo: None });
        assert_eq!(
            parse(&["tui", "--repo", "/work/repo"]).unwrap(),
            LiteCommand::Tui {
                repo: Some(PathBuf::from("/work/repo"))
            }
        );
        assert_eq!(
            parse(&["tasks", "."]).unwrap(),
            LiteCommand::Tui {
                repo: Some(PathBuf::from("."))
            }
        );
        assert!(parse(&["tui", "--remote", "box"]).is_err());
    }

    #[test]
    fn the_commands_without_a_window_are_the_full_moons() {
        assert_eq!(
            parse(&["tasks", "list"]).unwrap(),
            LiteCommand::Tasks(tasks::TaskCommand::List)
        );
        assert_eq!(
            parse(&["mcp"]).unwrap(),
            LiteCommand::Mcp(mcp::McpCommand::Serve { repo: None })
        );
        assert!(matches!(
            parse(&["wire", "post", "hello"]).unwrap(),
            LiteCommand::Wire(_)
        ));
        assert!(matches!(
            parse(&["agent", "list"]).unwrap(),
            LiteCommand::Agent(_)
        ));
        assert_eq!(parse(&[]).unwrap(), LiteCommand::Help);
        assert!(parse(&["review"]).is_err());
    }

    #[test]
    fn the_help_names_every_command() {
        let help = help_text();
        for said in [
            "tui",
            "tasks new",
            "wire post",
            "agent <command>",
            "mcp [--repo <path>]",
        ] {
            assert!(help.contains(said), "the help should say {said:?}");
        }
    }
}
