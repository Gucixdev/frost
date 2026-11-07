# frost

A small terminal text editor in Rust. Layout like [Fresh](https://github.com/sinelaw/fresh) (menu bar, tabs, gutter, status bar, command palette), keys like micro,
CUA clipboard, mouse, multi-cursor, and Notepad++-style sessions. About 2000 lines, 4 real dependencies.

    ./run                 # builds (release) and starts; or: cargo build --release
    frost [FILE...]       # no files: reopen the last session

## What it has
- **Keys (CUA + micro):** Ctrl-S/O/N/W/Q, Ctrl-Z/Y, Ctrl-X/C/V (system clipboard), Ctrl-A, Ctrl-F (live search) F3/Shift-F3, Ctrl-H replace all, Ctrl-G line, Ctrl-K delete line, Ctrl-Shift-D duplicate, Tab/Shift-Tab indent, Ctrl-P command palette, F1 keys, F10 menu.
- **Mouse:** click, drag, double-click word, triple-click line, wheel, tabs, menus (they open when you point at them, like Fresh), Alt/Ctrl-click adds a cursor.
- **Multi-cursor:** Ctrl-D selects the word, then the next occurrence each press; Alt-Up/Down adds a cursor above/below; every edit, paste (one clipboard line per cursor) and delete works at all cursors; Esc goes back to one.
- **Huge files:** a file over 8 MB shows its first screen at once (about 150 ms for 811 MB / 10 M lines on 12 cores) while all cores stream the rest in; the status bar shows `loading NN%`. Memory is about 1.4x the text, not 2x. Search works line by line, saving streams to disk. The first edit waits for the load to finish. Far jumps do not re-parse the highlighting. Not tried above ~2.4 GB.
- **Sessions (Notepad++ style):** open tabs, cursor, scroll, language *and unsaved or untitled text* are saved on quit (and every 30 s) and come back on the next start. Named sessions under File. Closed tabs stay reachable after a restart: Ctrl-T reopens the last one, File > Closed tabs... lists them.
- **Infinite undo/redo:** nothing is ever dropped; the whole history is written to the session on quit (compact deltas, texts up to 4 MB) and works after a restart.
- **Dictionary (TempleOS style):** while you type, words that start the same are offered with their meaning (Tab accepts, Up/Down choose, Esc closes); F2 explains the word under the caret. Sources: a built-in list, words near the caret, `~/.config/frost/dictionary.txt` (`word<TAB>meaning`) and `/usr/share/dict/words`.
- **Diff:** File > "Diff with file on disk" and "Diff with another tab..." open a unified diff in a tab (syntax-coloured `+`/`-`).
- **Languages:** 199 syntaxes, picked by extension, file name or shebang; Ctrl-L lists them A-Z with letter markers, typing filters (prefix matches first).
- **Config in Lua:** `~/.config/frost/init.lua`, see below.
- Line endings (LF/CRLF) are kept.

## Config (Lua)
```lua
frost.theme = "gruvbox"            -- dark (default), light, gruvbox, mono
frost.colors.accent = "#ff8800"    -- bg bar dim fg accent sel cursor2
frost.line_numbers = true
frost.tab_width = 4
frost.dictionary = "/path/to/words.tsv"
frost.keys["ctrl-j"] = "goto"      -- key spec -> action name (save, quit, undo, find, define, difftab, ...)
```
A broken file shows its error in the status line and the defaults are used. Syntax colours come from the bundled bat themes.

## Platforms
Linux is what it is developed and tested on. The code has no Linux-only parts: clipboard tries wl-copy, xclip, xsel, pbcopy, clip.exe and PowerShell; state and config go to XDG, `~/Library/Application Support` or `%APPDATA%`; big files are read with `read_at`/`seek_read`. macOS, Windows and the BSDs are **not tested yet**.

## Not here (on purpose)
No LSP, no plugin system beyond the Lua config, no file tree, no soft wrap, no split panes. It is meant to stay a small editor like micro, a bit better, not a VS Code clone.

## Tests
`./test` runs everything: the Lean 4 proofs in `lean4/Frost.lean` (applying the history delta of a to b gives b; only the axioms propext and Quot.sound), regenerates the 14641 oracle cases from the Lean model and checks the Rust `delta` against all of them, then `cargo test` (buffer, multi-cursor, lazy load, history, session, dictionary, diff, config).
The Lean part covers the undo-history delta only; the rest of the editor is covered by ordinary Rust tests.
