# 🌚 moontasks

`moontasks` is an agent multiplexer.

It lets you work with multiple agents while keeping track of which agent belongs to which task.

This project started as a code review tool (moonreview), with viewing diffs + sending feedback to agents still a core piece.

## Quick install

Install the latest prebuilt release:

```bash
curl -fsSL https://raw.githubusercontent.com/antoineMoPa/moontasks/main/install.sh | sh
```

This installs `moon` and the desktop launchers. If `~/.local/bin` is not already on
your `PATH`, add it in your shell configuration.

To take both away again, run `moon uninstall` - or pick **Window › Uninstall moon** twice.
Settings, boards and each repo's `.moontasks` folder are left as they are.

## Build from source

Requirements:

- [Rust](https://www.rust-lang.org/tools/install)
- [Zig](https://ziglang.org/) 0.15.x, for the window's terminal
- [the silver searcher](https://github.com/ggreer/the_silver_searcher) (`ag`), which is what
  finds files by name for `⌘P` and text in them for `⌘⇧F`

```bash
./scripts/setup-dev.sh            # Rust update, Zig and submodules
export PATH="$(brew --prefix zig@0.15)/bin:$PATH"
cargo install --locked --path .   # installs moon
moon install-launchers            # optional: launchers the OS itself offers
moon review
```

`--locked` builds the dependency versions in `Cargo.lock`; without it `cargo install` re-resolves
to the newest compatible versions, which is how you end up compiling a release nobody tested.

The window embeds Ghostty's terminal emulator
([libghostty-vt](https://ghostty.org/)), which is built from Ghostty's Zig
source, so a Zig 0.15.x toolchain has to be on `PATH` at build time. On macOS:

```bash
brew install zig@0.15
export PATH="$(brew --prefix zig@0.15)/bin:$PATH"
```

Everything still links statically - the result is one executable with no runtime dependency
on Zig or on a separate server process.

## Desktop launchers

`cargo install` leaves one executable on `PATH`, which is all a shell needs. To open the
windows from the OS as well - Spotlight and Launchpad on macOS, the application menu on Linux:

```bash
moon install-launchers
```

## Usage

```bash
moon tasks              # the sprint board
moon tui                # the sprint board, in a terminal
moon review             # review local changes
moon shell              # a shell in the folder
moon edit src/main.rs   # open a file for edition
moon edit src/          # pick a file of a folder to open
moon launch chromium    # in a shell of `moon serve` or `moon desktop`: start a program with windows
```

`moon edit --wait` returns only once the file's tab is closed, so git can write its commit
messages in a moon window:

```bash
git config --global core.editor "moon edit --wait"
```

### Moontasks

Task state lives in the repo's `.moontasks/` directory.

## Crates

Three pieces of the window are libraries in their own right, kept as submodules under
`crates/` and published separately:

- [**egui_frames**](crates/egui_frames) - tabs, splits and draggable panes for egui. The
  arrangement the Moon tools workspace is made of, with nothing product-specific in it.
- [**egui_tty**](crates/egui_tty) - a terminal emulator widget for egui, on Ghostty's VT engine.
  What a shell tab holds.
- [**egui_moon_editor**](crates/egui_moon_editor) - a code editor widget for egui: a text
  buffer, a fringe of line numbers that scrolls with the code and marks the lines new since the
  last commit, a column beside it for notes about stretches of lines - the blame a file tab
  puts up - and marks drawn into the text.
  What a file tab holds.

After cloning, pull them in:

```bash
git submodule update --init --recursive
```

## Development

I usually use this as part of my debug loop:

```bash
pkill moon;  cargo install --locked --path .
```

To install launchers:

```bash
cargo install --locked --path .; moon install-launchers
```

On mac you will need to drag applications from the Applications folder to your menu bar.

## Regenerating the docs screenshots

```bash
UPDATE_SNAPSHOTS=1 cargo test --lib -- native::ui_tests::board native::ui_tests::launch
```
