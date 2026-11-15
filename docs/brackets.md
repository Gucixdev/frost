# Brackets, quotes and indentation

How frost behaves while you type. Everything depends on the language of the tab (Ctrl-L), see `src/lang.rs`.

## Automatic pairs
- Typing `(` `[` `{` `"` `'` inserts the partner and leaves the caret between. Typing the closer in front of the same closer steps over it.
- A selection is wrapped instead of replaced and stays selected (`(` twice wraps twice).
- An opener is not paired in front of a word (`|foo` stays `(foo`); a quote is not paired after a word (`don't`), in front of a word, or when it closes a string.
- Backspace between an empty pair (`(|)`, `"|"`) removes both; between a filled pair only the one character.
- Enter between `{|}` (`(|)`, `[|]`) opens three lines: closer on its own line, the middle one indented.
- A closer typed as the first thing on a line (`}` after Enter) lines up with the line that opened it.
- All of it works at every cursor.

## Per language
| kind | pairs | notes |
|---|---|---|
| C-like, Python, Ruby, YAML, ... | `( [ { " '` | Rust has no `'` (lifetimes) |
| Lisp, Scheme, Clojure, Racket, Emacs Lisp | `( [ { "` | **strict**, no `'` (it is the quote) |
| Plain text, Markdown, LaTeX | `( [ { "` | no brackets are judged |

## Strict (Lisp)
An edit that turns balanced text into unbalanced text is undone and the status line says why: deleting half a form, typing a lone `)`, cutting a line that opens a form. Backspace and Delete step over a bracket instead of deleting it; an empty `()` goes away whole. Text that is already unbalanced can be edited freely, so it can be repaired.

Enter and Tab line up like Emacs: under the first argument of the innermost form, two columns in for `def...`, `let`, `lambda`, `when`, `with-...` and other body forms, one column in for lists and data. Tab re-indents the line (or all selected lines).

## Indentation
One level is what the file already does: tabs if most indented lines start with a tab, otherwise the most common step between indented lines (2, 3, 4 ... spaces). A file without indentation gets tabs in Go and Makefile and `frost.tab_width` spaces elsewhere. Enter keeps the indentation of the line, adds one level after an opening bracket (and after `:` in Python, YAML, GDScript, Nim). Tab inserts a tab or spaces up to the next stop; Shift-Tab removes one level.

## Seeing brackets
The bracket at the caret (or just before it) and its partner are bold and underlined. Brackets that are not closed, or have no opener, are red and underlined, and the status line says `unclosed ( line 12 (+2)` or `stray ) line 7`; when everything matches it shows `() ok`. Brackets inside strings, comments and character literals do not count. Languages where that cannot be told reliably (shell, HTML, JSON, ...) only get the matching, and prose gets nothing. Files over 512 KB are not scanned.

## init.lua
```lua
frost.auto_pair = false                 -- no automatic pairs at all
frost.pairs["*"] = "([{\""              -- opening characters that pair, for every other language
frost.pairs["rust"] = "([{\"'"          -- per language (lower case name as in the status line)
frost.strict["emacs lisp"] = false      -- lenient Lisp
frost.strict["clojure"] = true
```
