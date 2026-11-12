# moon tui - design

What the TUI is going for, so each change pulls the same way. The board is the product; the
chrome around it should barely be noticed.

1. **No hints on screen.** No key hints in footers, pane headings, menus or messages. Keys
   live in `?` (one short page) and in the command palette.
2. **Mouse first, keyboard complete.** Anything drawn that acts is clickable. Anything that
   can be done is a palette command, findable by its name.
3. **Clickable things look clickable.** Bracketed buttons (`[  +  ]`, `[start]`, `[x]`), `×`,
   `+` and `▾`. Every one of them lights under the pointer. Plain text is never a hidden button.

