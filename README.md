# frost

A small terminal text editor in Rust. Layout like [Fresh](https://github.com/sinelaw/fresh) (menu bar, tabs, gutter, status bar, command palette), keys like micro,
CUA clipboard, mouse, multi-cursor, and Notepad++-style sessions. About 2000 lines, 4 real dependencies.

    ./run                 # builds (release) and starts; or: cargo build --release
    frost [FILE...]       # no files: reopen the last session

## What it has
- **Keys (CUA + micro):** Ctrl-S/O/N/W/Q, Ctrl-Z/Y, Ctrl-X/C/V (system clipboard through wl-copy or xclip), Ctrl-A, Ctrl-F (live search) F3/Shift-F3, Ctrl-H replace all, Ctrl-G line, Ctrl-K delete line, Ctrl-Shift-D duplicate, Tab/Shift-Tab indent, Ctrl-P command palette, F1 keys, F10 menu.
- **Mouse:** click, drag, double-click word, triple-click line, wheel, tabs, menus, Alt/Ctrl-click adds a cursor.
- **Multi-cursor:** Ctrl-D selects the word, then the next occurrence each press; Alt-Up/Down adds a cursor on the line above/below; every edit, paste (one clipboard line per cursor) and delete works at all cursors; Esc goes back to one.
- **Sessions:** open tabs, cursor, scroll, language *and unsaved or untitled text* are saved on quit (and every 30 s) and restored on the next start. File > Save session as... / Load session... keep named ones.
- **Closed tabs:** File > Reopen closed tab (Ctrl-T) and Closed tabs...; the list (with unsaved text) survives restarts. Menus open on hover.
- **Languages:** 199 syntaxes (two-face's bat set), picked by extension, file name or shebang; Ctrl-L chooses one by hand.
- Undo is a snapshot of the rope (O(1) clone), typing is grouped. Line endings (LF/CRLF) are kept.

State lives in `$XDG_STATE_HOME/frost` (default `~/.local/state/frost`).

## Not here (on purpose, for now)
No LSP, no plugins, no file tree, no soft wrap, no split panes, UTF-8 only. This is the "basic usefulness" editor; the bigger Fresh fork and the C frost are separate projects.

## Tests
`cargo test` - buffer (multi-cursor, undo, search, CRLF), session (atomic save, corrupt file kept as .bad), highlight (languages, checkpoints).
