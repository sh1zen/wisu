# wisu

[![Build Status](https://github.com/sh1zen/wisu/actions/workflows/ci.yml/badge.svg)](https://github.com/sh1zen/wisu/actions)
[![Latest Version](https://img.shields.io/crates/v/wisu.svg)](https://crates.io/crates/wisu)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

### A Fast and minimalistic directory tree viewer, written in Rust, with a powerful interactive mode.

#### Inspired by [lstr]("https://github.com/bgreenwell/lstr");

![](assets/wisu-demo.gif)

## Features

- **Classic and interactive modes:** Use `wisu` for a classic `tree`-like view, or launch `wisu -i` for a fully
  interactive terminal Interface.
- **Watching mode:** in interactive mode is possible to enable real time update from filesystem using `--watch`
- **Theme-aware coloring:** Respects your system's `LS_COLORS` environment variable for fully customizable file and
  directory colors.
- **Rich information display (optional):**
    - Display file-specific icons with `--icons` (requires Emoji support).
    - Show file permissions with `-p`.
    - Show file sizes with `-s`.
    - Show file info with `-x`.
- **Export:** Export path to (`CSV`, `XML`, `JSON`) with `-o` flag.
- **Smart filtering:**
    - Respects your `.gitignore` files with the `-g` flag.
    - Control recursion depth (`-L`) or show only directories (`-d`).
    - Control max files per dir (`-F`), setting it to 0 displays only directories.
   - Time-based filtering with `-t` to show files modified within a time range.
- **Plugin support:**
    - You can customize wisu behavior using custom filtering with **apply_filter(hook, Fn);**

## Installation

### From source (all platforms)

You need the Rust toolchain installed on your system to build `wisu`.

1. **Clone the repository:**
   ```bash
   git clone https://github.com/sh1zen/wisu.git
   cd wisu
   ```
2. **Build and install using Cargo:**
   ```bash
   cargo install --path .
   ```

## Usage

```bash
wisu [PATH] [OPTIONS]
```

Note that `PATH` defaults to the current directory (`.`) if not specified.

| Option                   | Description                                                                                               |
|:-------------------------|:----------------------------------------------------------------------------------------------------------|
| `-i`                     | Enable interactive mode (see below).                                                                      |
| `--watch`                | Enable watching mode (interactive mode only).                                                             |
| `--config <PATH>`        | Loads configuration from a TOML file.                                                                     |
| `-o <TYPE>`              | Export to file. TYPE: (`csv`, `xml`, `json`).                                                             |
| `-a`, `--all`            | List all files and directories, including hidden ones.                                                    |
| `-d`, `--dirs-only`      | List directories only, ignoring all files.                                                                |
| `-g`, `--gitignore`      | Respect `.gitignore` and other standard ignore files.                                                     |
| `--exclude <EXTS>`       | Exclude files by extension (comma-separated, e.g. `log,tmp`).                                             |
| `-t`, `--time <FILTER>`  | Filter files by modification time (see [Time filtering](#time-filtering)).                                |
| `-L`, `--level <LEVEL>`  | Maximum depth to descend.                                                                                 |
| `-F`, `--files <NUM>`    | List max NUM files per directory.                                                                         |
| `--expand-level <LEVEL>` | **Interactive mode only:** Initial depth to expand the interactive tree.                                  |
| `--sort <TYPE>`          | Sort entries by the specified criteria (`name`, `size`, `accessed`, `created`, `modified`, `extension`).  |
| `--dirs-first`           | Sort directories before files.                                                                            |
| `--case-sensitive`       | Use case-sensitive sorting.                                                                               |
| `--natural-sort`         | Use natural/version sorting (e.g., file1 < file10).                                                       |
| `-r`, `--reverse`        | Reverse the sort order.                                                                                   |
| `--dotfiles-first`       | Sort dotfiles and dot-folders first (dot-folders → folders → dotfiles → files).                           |
| `--icons`                | Display file-specific icons using emoji.                                                                  |
| `--hyperlinks`           | Render file paths as clickable hyperlinks (classic mode only).                                            |
| `-s`, `--size`           | Display just files size.                                                                                  |
| `-p`, `--permissions`    | Display file permissions (Unix-like systems only).                                                        |
| `-x`, `--info`           | Display files and directories info.                                                                       |

-----

## Time filtering

The `-t` / `--time` option filters files based on their modification time. It supports both **relative** and **absolute** time filters.

### Relative time (files modified within the last...)

| Unit | Description |
|:-----|:------------|
| `s`  | Seconds     |
| `m`  | Minutes     |
| `h`  | Hours       |
| `d`  | Days        |
| `w`  | Weeks       |
| `M`  | Months      |
| `y`  | Years       |

**Examples:**
```bash
wisu -t 30s      # Files modified in the last 30 seconds
wisu -t 10m      # Files modified in the last 10 minutes
wisu -t 2h       # Files modified in the last 2 hours
wisu -t 5d       # Files modified in the last 5 days
wisu -t 2w       # Files modified in the last 2 weeks
wisu -t 3M       # Files modified in the last 3 months
wisu -t 1y       # Files modified in the last year
```

### Absolute date (files modified before/after a specific date)

Supported date formats: `dd-mm-yyyy`, `dd/mm/yyyy`, `yyyy-mm-dd`

| Prefix | Description                    |
|:-------|:-------------------------------|
| (none) | Files modified **after** date  |
| `>`    | Files modified **after** date  |
| `<`    | Files modified **before** date |

**Examples:**
```bash
wisu -t 01-06-2024       # Files modified after June 1st, 2024
wisu -t 01/06/2024       # Same as above (alternative format)
wisu -t 2024-06-01       # Same as above (ISO format)
wisu -t "<01-01-2023"    # Files modified before January 1st, 2023
wisu -t ">15/03/2024"    # Files modified after March 15th, 2024
```

> **Note:** When using `<` or `>` prefixes, wrap the argument in quotes to prevent shell interpretation.


## Interactive mode

### Search

 - With `/` classic search mode.
 - With `/r:` regex search mode.

### Keyboard & Mouse controls

| Key(s)      | Action                                                                                                                                      |
|:------------|:--------------------------------------------------------------------------------------------------------------------------------------------|
| `↑`         | Move selection up.                                                                                                                          |
| `↓`         | Move selection down.                                                                                                                        |
| `Scroll`    | Mouse scroll support                                                                                                                        |                                                                                                                        
| `Enter`     | **Context-aware action:**\<br\>- If on a file: Open it in the default editor (`$EDITOR`).\<br\>- If on a directory: Toggle expand/collapse. |
| `q` / `Esc` | Quit the application normally.                                                                                                              | 
| `r`         | Refresh the tree view.                                                                                                                      |
| `Ctrl`+`s`  | **Shell integration:** Quits and prints the selected path to stdout.                                                                        |
| `Ctrl`+`t`  | **Shell integration:** Open a terminal in the selected directory.                                                                           |

## Customization

Supporting plugins as a hook filtering.

use add_filter("hook", |a| { a }); to customize some behaviour.

```
pub fn add_filter<T>(
    hook: impl Into<String>,
    filter: impl Fn(T) -> T + Send + Sync + 'static,
) where T: Any + Send + 'static;
```

## Examples

**1. List the contents of the current directory**

```bash
wisu
```

**2. Explore a project interactively, ignoring gitignored files**

```bash
wisu interactive -g --icons
```

**3. Get a tree with clickable file links (in a supported terminal)**

```bash
wisu --hyperlinks
```

**4. Start an interactive session**

```bash
wisu -i --icons -s -p
```

**5. Sort files naturally with directories first**

```bash
wisu --dirs-first --natural-sort
```

**6. Sort by file size in descending order**

```bash
wisu --sort size --reverse
```

**7. Sort by extension with case-sensitive ordering**

```bash
wisu --sort extension --case-sensitive
```

**8. Sort with dotfiles first and directories first**

```bash
wisu --dotfiles-first --dirs-first -a
```

## Piping and shell interaction

The classic `view` mode is designed to work well with other command-line tools via pipes (`|`).

### Interactive fuzzy finding with `fzf`

This is a powerful way to instantly find any file in a large project.

```bash
wisu -a -g --icons | fzf
```

`fzf` will take the tree from `wisu` and provide an interactive search prompt to filter it.

### Paging large trees with `less`

If a directory is too large to fit on one screen, pipe the output to a *pager*.

```bash
# Using less (the -R flag preserves color)
wisu -L 10 | less -R
```

### Changing directories with `wisu`

You can use `wisu` as a visual `cd` command. Add the following function to your shell's startup file (e.g., `~/.bashrc`,
`~/.zshrc`):

```bash
# A function to visually change directories with wisu
chdir() {
    # Run wisu and capture the selected path into a variable.
    # The TUI will draw on stderr, and the final path will be on stdout.
    local selected_dir
    selected_dir="$(wisu interactive -g --icons)"

    # If the user selected a path (and didn't just quit), `cd` into it.
    # Check if the selection is a directory.
    if [[ -n "$selected_dir" && -d "$selected_dir" ]]; then
        cd "$selected_dir"
    fi
}
```

After adding this and starting a new shell session (or running `source ~/.bashrc`), you can simply run:

```bash
chdir
```

This will launch the `wisu` interactive UI. Navigate to the directory you want, press `Ctrl+s`, and your shell's current
directory will instantly change.

## Color customization

`wisu` respects your terminal's color theme by default. It reads the `LS_COLORS` environment variable to colorize files
and directories according to your system's configuration. This is the same variable used by GNU `ls` and other modern
command-line tools.

### Windows

Windows does not use the `LS_COLORS` variable natively, but you can set it manually to enable color support in modern
terminals like Windows Terminal.

To set it for your current **PowerShell** session, run:

```powershell
$env:LS_COLORS="rs=0:di=01;33:ln=01;35:ex=01;36:*.zip=01;32:*.png=01;31:"
```

To set it for your current **Command Prompt** (cmd) session, run:

```cmd
set LS_COLORS=rs=0:di=01;33:ln=01;35:ex=01;36:*.zip=01;32:*.png=01;31:
```

After setting the variable and starting a new shell session, `wisu` will automatically display your configured colors.

## License

This project is licensed under the terms of the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).

