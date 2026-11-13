---
name: moontasks-interaction
description: Moontasks interaction - read and change the moon task board of a repo with this server's tools. Use when asked to list, find, create, move, rename or tag tasks or cards, read or write a task's notes, link a tracker issue, read or post on the wire (the board's messageboard), or list, start, view or message the agents running on the board.
---

# Moontasks interaction

The board of a repo is the `.moontasks/` folder at its root: columns of cards, each card a
task with a folder of its own. This server's tools read and change it.

## Rules that save round trips

- **A task is named by its handle**: `fix-the-races`, as `tasks_list` prints it (the `@` is
  optional). A handle does not change when the task is renamed.
- **Leave `task` out for your own task** when this server was started from a task's shell.
  Otherwise name it: a call that leaves it out is refused with "which task?".
- **Columns go by the name the board shows**, letter for letter: `IN PROGRESS`, not
  `in_progress`. A wrong name is refused with the list of the real ones.
- **One `task_update` call can do everything**: title, column, tags, notes, tracker link.
- **Prefer `append_notes` to `notes`**: `notes` replaces what a person wrote.
- **A refusal is text to read**: it says what was wrong, and usually what to do instead.

## Tools

| Want | Tool | Arguments |
| --- | --- | --- |
| See the board | `tasks_list` | `column`, `tag`, `matching` (title text), `limit` (50 a column) |
| Read one task whole | `task_read` | `task` |
| Make a card | `task_create` | `title`, then `column`, `tags`, `notes`, `appears_on` (`2026-11-02`) |
| Change a card | `task_update` | `task`, then any of `title`, `column`, `tags`, `add_tags`, `remove_tags`, `notes`, `append_notes`, `tracker_url` |
| Read the wire | `wire_read` | `lines` (20) |
| Post on the wire | `wire_post` | `line` - start it with `@handle` to reach one task's agents |
| Who is running | `agents_list` | - |
| Start an agent | `agent_start` | `task`, `agent` (`pi`, `claude`, `codex`, `minerve`, `opencode`) |
| See an agent | `agent_view` | `task`, `lines` (its last n lines rather than its screen) |
| Message an agent | `agent_tell` | `task`, `line` |

## Things to know

- The finished column (DONE) is only counted in `tasks_list`. Ask for it with
  `column: "DONE"`, or search it with `tag` or `matching`.
- `wire_post` needs a sender, so it works only from a task's shell. `wire_read` works anywhere.
- `agent_start` needs a moon window open on the repo, and starts a real, paid agent: do it
  when asked, not to explore.
- `agent_tell` types into another agent's input. One line, and say who you are.
- Before changing code on a shared board: `wire_read`, then `wire_post` the areas you will
  touch, and post again when done.
- No tool deletes a task, edits columns, links sub-tasks or edits attachments. To attach a
  file, add its path as a line of `file_attachments.txt` in the task's folder - `task_read`
  says where the folder is.
- The board is local: the one of the repo this server was started in, or of
  `moon mcp --repo <path>`.

## Typical flows

- **"What should I work on?"** `tasks_list`, then `task_read` the top card of the first column.
- **Starting a task**: `task_update` with `column: "IN PROGRESS"`, `wire_read`, `wire_post`.
- **Progress**: `task_update` with `append_notes`.
- **Finishing**: write a review file per PR (see `.moontasks/llms/request_review.md`),
  `wire_post` that it is done, and move the card only if asked to.
- **Delegating**: `task_create`, `agent_start`, `agent_tell` the brief, then `agent_view`
  with `lines` to check on it.
