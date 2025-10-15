use crate::app::Args;
use crate::common::{icons, tree};
use crate::utils::{dir, format};
use colored::Colorize;
use lscolors::LsColors;
use std::fs;
use std::io::{self, Write};
use std::time::Instant;
use url::Url;

/// Runs the classic directory tree view
pub fn run(args: &Args, ls_colors: &LsColors) -> anyhow::Result<()> {
    let start_time = Instant::now();

    // ─────────────── Data preparation ───────────────
    let tree = tree::Tree::prepare(args, true)?;

    // ─────────────── Print ───────────────
    let (dir_count, file_count, size) = print_tree(tree, ls_colors, args)?;

    let elapsed = start_time.elapsed();

    if args.stats {
        writeln!(
            io::stdout(),
            "\n{}, {dir_count} directories, {file_count} files ( {:.2?} )",
            format::size(size),
            elapsed
        )?;
    }

    Ok(())
}

pub fn print_tree(
    tree: tree::Tree,
    ls_colors: &LsColors,
    args: &Args,
) -> anyhow::Result<(usize, usize, u64)> {
    // ───────────── ROOT ─────────────
    let metadata = fs::metadata(&args.path).ok();
    let root_is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(true);

    let root_icon = if args.icons {
        format!("{} ", icons::get_icon_for_path(&args.path, root_is_dir))
    } else {
        String::new()
    };

    let root_permissions =
        if args.permissions { dir::get_permission(metadata) } else { String::new() };

    let root_entries = tree.entries_at_depth(1);

    let root_size: u64 = root_entries.iter().map(|(_, info)| info.size.unwrap_or(0)).sum();

    let root_size_str = if args.info || args.size {
        format!(
            " ( {}  {} dirs, {} files )",
            format::size(root_size),
            root_entries
                .iter()
                .filter(|(entry, _)| entry.file_type().is_some_and(|ft| ft.is_dir()))
                .count(),
            root_entries
                .iter()
                .filter(|(entry, _)| entry.file_type().is_some_and(|ft| !ft.is_dir()))
                .count()
        )
    } else {
        String::new()
    };

    writeln!(
        io::stdout(),
        "{}{}{}{}",
        root_permissions.dimmed(),
        root_icon,
        args.path.display().to_string().blue().bold(),
        root_size_str.dimmed()
    )?;

    // ───────────── ENTRIES ─────────────
    let mut dir_count = 0usize;
    let mut file_count = 0usize;
    let mut path_stack: Vec<bool> = Vec::new();

    for (i, entry) in tree.entries.iter().enumerate() {
        let c_info = &tree.tree_info[i];
        let depth = c_info.depth;

        // Aggiorna stack in base alla profondità
        while path_stack.len() >= depth {
            path_stack.pop();
        }
        path_stack.push(c_info.connector == "└──");

        let mut prefix = String::new();
        for &is_last in &path_stack[..path_stack.len() - 1] {
            prefix.push_str(if is_last { "    " } else { "│   " });
        }

        // Conteggi
        if c_info.is_directory {
            dir_count += 1;
        } else {
            file_count += 1;
        }

        let size_str = if args.info {
            if c_info.is_directory {
                format!(
                    "  [ {}  {} dirs, {} files ]",
                    format::size(c_info.size.unwrap_or(0)),
                    c_info.dirs.unwrap_or(0),
                    c_info.files.unwrap_or(0)
                )
            } else {
                format!("  [ {} ]", format::size(c_info.size.unwrap_or(0)))
            }
        } else if args.size && !c_info.is_directory {
            c_info.size.map(|s| format!(" ({})", format::size(s))).unwrap_or_default()
        } else {
            String::new()
        };

        let styled_name = style_entry_name(entry.path(), ls_colors);
        let final_name = if args.hyperlinks && !c_info.is_directory {
            make_hyperlink(entry.path(), styled_name)
        } else {
            styled_name.to_string()
        };

        writeln!(
            io::stdout(),
            "{}{}{} {}{}{}",
            c_info.permissions.clone().unwrap_or_default().dimmed(),
            prefix,
            c_info.connector,
            c_info.icon.clone().unwrap_or_default(),
            final_name,
            size_str.dimmed()
        )?;
    }

    Ok((dir_count, file_count, root_size))
}

#[inline]
fn style_entry_name(path: &std::path::Path, ls_colors: &LsColors) -> colored::ColoredString {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    // safe metadata
    let metadata = fs::metadata(path).ok();

    // Default color based on type/extension
    let mut styled = if let Some(metadata) = &metadata {
        if metadata.is_dir() {
            name.blue().bold()
        } else if is_executable(path, metadata) {
            name.green()
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_lowercase().as_str() {
                "rs" | "c" | "cpp" | "py" | "php" | "html" | "css" | "js" => name.cyan(), // source files
                "zip" | "tar" | "gz" | "rar" | "7zip" => name.yellow(), // archives
                "psd" | "svg" | "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" => name.magenta(), // images
                "mp4" | "mkv" | "avi" | "mov" | "flv" | "wmv" => name.purple(), // videos
                "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pps" | "ppsx" => {
                    name.bright_black()
                }
                _ => name.white(),
            }
        } else {
            name.normal()
        }
    } else {
        name.normal()
    };

    // LS colors always take precedence
    if let Some(ls_style) = ls_colors.style_for_path(path) {
        let mut ls_styled = styled.normal();

        if let Some(fg) = ls_style.foreground {
            ls_styled = ls_styled.color(ls_color_to_colored(fg));
        }

        if ls_style.font_style.bold {
            ls_styled = ls_styled.bold();
        }
        if ls_style.font_style.italic {
            ls_styled = ls_styled.italic();
        }
        if ls_style.font_style.underline {
            ls_styled = ls_styled.underline();
        }
        styled = ls_styled;
    }

    styled
}

// Cross-platform function to check if a file is executable
#[inline]
fn is_executable(path: &std::path::Path, metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        let _ = path;
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(windows)]
    {
        let _ = metadata;
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext.to_lowercase().as_str(), "exe" | "bat" | "cmd"))
            .unwrap_or(false)
    }
}

#[inline]
fn ls_color_to_colored(ls_color: lscolors::Color) -> colored::Color {
    use lscolors::Color as LsColor;
    match ls_color {
        LsColor::Black => colored::Color::Black,
        LsColor::Red => colored::Color::Red,
        LsColor::Green => colored::Color::Green,
        LsColor::Yellow => colored::Color::Yellow,
        LsColor::Blue => colored::Color::Blue,
        LsColor::Magenta => colored::Color::Magenta,
        LsColor::Cyan => colored::Color::Cyan,
        LsColor::White => colored::Color::White,
        LsColor::BrightBlack => colored::Color::BrightBlack,
        LsColor::BrightRed => colored::Color::BrightRed,
        LsColor::BrightGreen => colored::Color::BrightGreen,
        LsColor::BrightYellow => colored::Color::BrightYellow,
        LsColor::BrightBlue => colored::Color::BrightBlue,
        LsColor::BrightMagenta => colored::Color::BrightMagenta,
        LsColor::BrightCyan => colored::Color::BrightCyan,
        LsColor::BrightWhite => colored::Color::BrightWhite,
        LsColor::Fixed(_) => colored::Color::White,
        // Fallback for fixed colors
        LsColor::RGB(r, g, b) => colored::Color::TrueColor { r, g, b },
    }
}

// Create a clickable hyperlink (if supported by the terminal)
fn make_hyperlink(path: &std::path::Path, styled_name: colored::ColoredString) -> String {
    if let Ok(abs_path) = fs::canonicalize(path) {
        if let Ok(url) = Url::from_file_path(abs_path) {
            return format!("\x1B]8;;{url}\x07{styled_name}\x1B]8;;\x07");
        }
    }
    styled_name.to_string()
}
