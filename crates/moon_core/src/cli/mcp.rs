//! `moon mcp`: the board of a repo, served to an agent over the Model Context Protocol.
//!
//! What `moon tasks new|list|move`, `moon wire` and `moon agent` do from a shell, as tools an
//! agent calls without one - and the rest of what a card has written on it, which a shell
//! reads and writes as files: the notes, the tags, the title, the tracker link. See
//! [`tools`] for the tools and [`help_text`] for what somebody setting it up is told.
//!
//! It speaks over this process's stdin and stdout, a JSON-RPC message per line, which is the
//! transport every MCP client starts a local server on. Nothing here may print to stdout
//! but the answers.
//!
//! Local only: the board is the folder of files in a repo on this machine. A board on
//! another machine - `moon tasks --remote` - is not reached.

mod board;
mod card_tools;
mod protocol;
mod skills;
#[cfg(test)]
mod tests;
mod tools;

use std::{
    env,
    io::{self, BufRead, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};

use super::PROGRAM;
use board::Board;

/// What `moon mcp …` asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum McpCommand {
    Help,
    /// The server, on the board of the repo this folder is in - or of the one the command
    /// was run in, when none is named.
    Serve {
        repo: Option<String>,
    },
}

/// The option that names the repo whose board is served, for a client that starts its
/// servers in a folder of its own choosing.
const REPO_OPTION: &str = "--repo";

/// Which of the commands a command line names, from the words after `mcp`.
pub fn parse(args: &[String], asks_for_help: bool) -> Result<McpCommand> {
    if asks_for_help {
        return Ok(McpCommand::Help);
    }
    let repo = match args {
        [] => None,
        [option, repo] if option == REPO_OPTION => Some(repo.clone()),
        _ => bail!(
            "`{PROGRAM} mcp` takes `{REPO_OPTION} <path>` and nothing else\n\n{}",
            help_text()
        ),
    };
    Ok(McpCommand::Serve { repo })
}

pub fn run(command: McpCommand) -> Result<()> {
    match command {
        McpCommand::Help => {
            println!("{}", help_text());
            Ok(())
        }
        McpCommand::Serve { repo } => {
            let folder = match repo {
                Some(repo) => PathBuf::from(repo),
                None => env::current_dir().context("failed to read the current directory")?,
            };
            let board = Board::of_folder(&folder)?;
            serve_messages(&board, io::stdin().lock(), io::stdout().lock())
        }
    }
}

/// Answer every message a client writes, a line each, until it closes its end.
fn serve_messages(board: &Board, messages: impl BufRead, mut answers: impl Write) -> Result<()> {
    for line in messages.lines() {
        let line = line.context("failed to read from the MCP client")?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = protocol::answer_message(board, &line) {
            writeln!(answers, "{answer}").context("failed to answer the MCP client")?;
            answers.flush().context("failed to answer the MCP client")?;
        }
    }
    Ok(())
}

/// `moon mcp --help`: what it takes to connect a client, and no more - the tools are the
/// server's own to say, to the client that asks it.
pub fn help_text() -> String {
    format!(
        "{PROGRAM} mcp

Serves this repo's task board to an agent: an MCP server on stdin and stdout.

Usage:
  {PROGRAM} mcp                   the board of the repo it is run in
  {PROGRAM} mcp {REPO_OPTION} <path>     the board of that repo

Connect:
  claude mcp add moon -- {PROGRAM} mcp
  codex mcp add moon -- {PROGRAM} mcp

Any other client, in its MCP config:
  {{ \"mcpServers\": {{ \"moon\": {{ \"command\": \"{PROGRAM}\", \"args\": [\"mcp\", \"{REPO_OPTION}\", \"/path/to/repo\"] }} }} }}"
    )
}
