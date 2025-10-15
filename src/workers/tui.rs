use crate::app::Args;
use crate::common::tree::{Tree, TreeEntry};
use crate::utils::dir::canonicalize_path;
use crate::utils::format;
use lscolors::{Color as LsColor, LsColors, Style as LsStyle};
use ratatui::crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, KeyEventKind, MouseEventKind,
};
use ratatui::crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend}, layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
    Terminal,
};
use std::io::{stdout, Stdout};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

/// TUI modes: normal navigation vs search mode
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Search,
}

/// Wrapper around TreeEntry to store expansion state for directories
#[derive(Clone)]
struct TuiEntry {
    data: TreeEntry,
    expanded: bool,
}

/// Represents what to do when exiting the TUI
enum ExitAction {
    None,
    PrintPath(PathBuf),
}

/// Main TUI application state
pub struct TuiApp {
    // All entries in the current view
    entries: Vec<TuiEntry>,
    // Indices of currently visible entries
    filtered_indices: Vec<usize>,
    // Tracks selection in the list
    list_state: ListState,
    // Current mode (Normal/Search)
    mode: Mode,
    // Search query string
    search_query: String,
    // Backup of indices before search
    backup_indices: Vec<usize>,
    // Currently displayed directory
    current_dir: PathBuf,
    root_dir: PathBuf,
}

impl TuiApp {
    pub fn new(entries: Vec<TreeEntry>, current_dir: impl Into<PathBuf>) -> Self {
        let current_dir = current_dir.into();
        let entries: Vec<TuiEntry> =
            entries.into_iter().map(|e| TuiEntry { data: e, expanded: false }).collect();

        let mut app = Self {
            entries,
            filtered_indices: Vec::new(),
            list_state: ListState::default(),
            mode: Mode::Normal,
            search_query: String::new(),
            backup_indices: Vec::new(),
            current_dir: current_dir.clone(),
            root_dir: current_dir, // <- qui impostiamo il root
        };
        app.rebuild_visible_list();
        app
    }

    pub fn apply_initial_expansion(&mut self, expand_level: Option<usize>) {
        if let Some(level) = expand_level {
            for entry in &mut self.entries {
                if entry.data.is_directory && entry.data.depth < level {
                    entry.expanded = true;
                }
            }
            self.rebuild_visible_list();
        }
    }

    /// Ricostruisce la lista dei nodi visibili in base alla directory corrente
    pub fn rebuild_visible_list(&mut self) {
        self.filtered_indices.clear();
        let mut parent_expanded_stack = Vec::with_capacity(16);

        for (idx, entry) in self.entries.iter().enumerate() {
            // ".." è sempre visibile se non siamo alla root
            if entry.data.icon.as_deref() == Some("..") {
                self.filtered_indices.push(idx);
                continue;
            }

            if self.current_dir == self.root_dir {
                // Logica albero completo con espansioni
                let target_depth = entry.data.depth.saturating_sub(1);
                parent_expanded_stack.truncate(target_depth);
                let visible = entry.data.depth == 0 || parent_expanded_stack.iter().all(|&e| e);
                if visible {
                    self.filtered_indices.push(idx);
                }
                if entry.data.is_directory && entry.data.depth > 0 {
                    parent_expanded_stack.push(entry.expanded);
                }
            } else {
                // Subdir: mostra solo figli diretti della current_dir
                if entry.data.path.parent().map(|p| p == self.current_dir).unwrap_or(false) {
                    self.filtered_indices.push(idx);
                }
            }
        }
        self.list_state.select(Some(0));
    }

    pub fn toggle_expansion(&mut self) {
        let Some(sel_idx) = self.list_state.selected() else { return };
        let Some(&entry_idx) = self.filtered_indices.get(sel_idx) else { return };
        if !self.entries[entry_idx].data.is_directory {
            return;
        }

        let path = self.entries[entry_idx].data.path.clone();
        self.entries[entry_idx].expanded = !self.entries[entry_idx].expanded;
        self.rebuild_visible_list();

        let new_pos = self
            .filtered_indices
            .iter()
            .position(|&i| self.entries[i].data.path == path)
            .unwrap_or_else(|| sel_idx.min(self.filtered_indices.len().saturating_sub(1)));
        self.list_state.select(Some(new_pos));
    }

    #[inline]
    fn move_selection_down(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let next = self
            .list_state
            .selected()
            .map(|i| if i >= self.filtered_indices.len() - 1 { 0 } else { i + 1 })
            .unwrap_or(0);
        self.list_state.select(Some(next));
    }

    #[inline]
    fn move_selection_up(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let prev = match self.list_state.selected() {
            Some(0) => self.filtered_indices.len() - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.list_state.select(Some(prev));
    }

    fn start_search(&mut self) {
        if self.mode == Mode::Normal {
            self.backup_indices = self.filtered_indices.clone();
        }
        self.mode = Mode::Search;
        self.search_query.clear();
    }

    fn exit_search(&mut self) {
        if self.mode != Mode::Search {
            return;
        }
        std::mem::swap(&mut self.filtered_indices, &mut self.backup_indices);
        self.backup_indices.clear();
        self.mode = Mode::Normal;
        self.search_query.clear();

        if let Some(sel) = self.list_state.selected() {
            if sel >= self.filtered_indices.len() {
                let new_sel = if self.filtered_indices.is_empty() {
                    None
                } else {
                    Some(self.filtered_indices.len() - 1)
                };
                self.list_state.select(new_sel);
            }
        }
    }

    fn apply_search_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            self.rebuild_visible_list();
            return;
        }

        self.entries.iter_mut().for_each(|e| e.expanded = false);
        self.filtered_indices.clear();

        for idx in 0..self.entries.len() {
            let name = self.entries[idx]
                .data
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if name.contains(&query) {
                self.filtered_indices.push(idx);
                let mut depth = self.entries[idx].data.depth;
                let mut parent_idx = idx;
                while depth > 0 {
                    if let Some(p_idx) =
                        (0..parent_idx).rev().find(|&i| self.entries[i].data.depth == depth - 1)
                    {
                        parent_idx = p_idx;
                        self.entries[parent_idx].expanded = true;
                        depth -= 1;
                    } else {
                        break;
                    }
                }
            }
        }
        self.list_state.select(if self.filtered_indices.is_empty() { None } else { Some(0) });
    }

    #[inline]
    fn get_current_entry(&self) -> Option<&TuiEntry> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered_indices.get(i))
            .and_then(|&idx| self.entries.get(idx))
    }

    pub fn enter_directory(&mut self, entry_idx: usize) {
        let entry = &self.entries[entry_idx];
        if !entry.data.is_directory {
            return;
        }

        self.current_dir = entry.data.path.clone();

        // Rimuovi eventuale ".." precedente
        self.entries.retain(|e| e.data.icon.as_deref() != Some(".."));

        // Aggiungi ".." solo se non siamo nella root base
        if self.current_dir != self.root_dir {
            if let Some(parent) = self.current_dir.parent() {
                let back_entry = TuiEntry {
                    data: TreeEntry {
                        path: parent.to_path_buf(),
                        depth: 0,
                        is_directory: true,
                        size: None,
                        files: None,
                        dirs: None,
                        icon: Some("..".to_string()),
                        permissions: None,
                        connector: String::new(),
                    },
                    expanded: false,
                };
                self.entries.insert(0, back_entry);
            }
        }

        self.rebuild_visible_list();
    }

    pub fn go_up(&mut self) {
        // Se siamo nella root base, non salire sopra
        if self.current_dir == self.root_dir {
            return;
        }

        // Trova l’indice del nodo ".." e entra nella directory superiore
        if let Some(parent) = self.current_dir.parent() {
            if let Some(back_idx) = self.entries.iter().position(|e| e.data.path == parent) {
                self.enter_directory(back_idx);
            }
        }
    }

    /// Render the TUI
    pub fn render<B: Backend>(&mut self, f: &mut Frame, args: &Args, ls_colors: &LsColors)
    where
        <B as Backend>::Error: Send + Sync + 'static,
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // breadcrumb
                Constraint::Min(1),    // list
                Constraint::Length(1), // status bar
            ])
            .split(f.area());

        // Breadcrumb path at the top
        let breadcrumb = Paragraph::new(self.current_dir.display().to_string())
            .style(Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC));
        f.render_widget(breadcrumb, chunks[0]);

        // Prepare list items
        let mut list_items = Vec::with_capacity(self.filtered_indices.len());

        for &idx in &self.filtered_indices {
            let entry = &self.entries[idx];
            let mut spans = Vec::with_capacity(6);

            if args.permissions {
                if let Some(perm) = &entry.data.permissions {
                    spans.push(Span::styled(
                        format!("{perm} "),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }

            if entry.data.depth > 0 {
                spans.push(Span::raw("    ".repeat(entry.data.depth)));
            }

            let indicator = if entry.data.is_directory {
                if entry.expanded { "▼ " } else { "▶ " }
            } else {
                "  "
            };
            spans.push(Span::raw(indicator));

            if let Some(icon) = &entry.data.icon {
                spans.push(Span::styled(format!("{icon} "), Style::default().fg(Color::Gray)));
            }

            let name = entry.data.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();

            let style = ls_colors.style_for_path(&entry.data.path).cloned().unwrap_or_default();

            spans.push(Span::styled(name.to_string(), convert_ls_style(style)));

            // Optional info aligned to the right
            let mut info_text = String::new();

            if args.info {
                if entry.data.is_directory {
                    if let (Some(size), Some(files), Some(dirs)) =
                        (entry.data.size, entry.data.files, entry.data.dirs)
                    {
                        info_text =
                            format!("[{}, {} files, {} dirs]", format::size(size), files, dirs);
                    }
                } else if let Some(size) = entry.data.size {
                    info_text = format!("[{}]", format::size(size));
                }
            } else if args.size && !entry.data.is_directory {
                if let Some(size) = entry.data.size {
                    info_text = format!("[{}]", format::size(size));
                }
            }

            if !info_text.is_empty() {
                let used_width: usize = spans.iter().map(|s| s.width()).sum();
                let padding = chunks[1]
                    .width
                    .saturating_sub(used_width as u16)
                    .saturating_sub(info_text.len() as u16)
                    .saturating_sub(5) as usize;

                if padding > 0 {
                    spans.push(Span::raw(" ".repeat(padding)));
                }
                spans.push(Span::styled(info_text, Style::default().fg(Color::DarkGray)));
            }

            list_items.push(ListItem::new(Line::from(spans)));
        }

        let list = List::new(list_items)
            .block(Block::default().title("Directory Tree").borders(Borders::ALL))
            .highlight_style(
                Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("→ ");

        f.render_stateful_widget(list, chunks[1], &mut self.list_state);

        // Status bar with instructions or search query
        let status_text = match self.mode {
            Mode::Normal => Span::styled(
                "q: quit | /: search | r: refresh | Tab: enter dir | Ctrl+T: open terminal | Ctrl+S: print path",
                Style::default().fg(Color::Gray),
            ),
            Mode::Search => Span::styled(
                format!("/{}", self.search_query),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        };
        f.render_widget(Paragraph::new(Line::from(status_text)), chunks[2]);
    }
}

/// Run the TUI application
pub fn run(args: &Args, ls_colors: &LsColors) -> anyhow::Result<()> {
    let entries = Tree::prepare(args, true)?.tree_info;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    while event::poll(std::time::Duration::from_millis(50))? {
        let _ = event::read()?;
    }

    let mut app = TuiApp::new(entries, args.path.clone());
    app.apply_initial_expansion(args.expand_level);

    let exit_action = loop {
        terminal.draw(|f| app.render::<CrosstermBackend<Stdout>>(f, args, ls_colors))?;

        let Event::Key(key) = event::read()? else {
            if let Event::Mouse(mouse) = event::read()? {
                match mouse.kind {
                    MouseEventKind::ScrollUp => app.move_selection_up(),
                    MouseEventKind::ScrollDown => app.move_selection_down(),
                    _ => {}
                }
            }
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        if app.mode == Mode::Search && !key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Backspace => {
                    app.search_query.pop();
                    app.apply_search_filter();
                }
                KeyCode::Char(c) => {
                    app.search_query.push(c);
                    app.apply_search_filter();
                }
                KeyCode::Esc => app.exit_search(),
                KeyCode::Enter => {
                    if let Some(entry) = app.get_current_entry() {
                        if entry.data.is_directory {
                            app.toggle_expansion();
                        } else {
                            let _ = open_file(&entry.data.path);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        match key.code {
            KeyCode::Char('q') => break ExitAction::None,
            KeyCode::Char('r') => {
                terminal.clear()?;
                let new_entries = Tree::prepare(args, true)?.tree_info;
                app = TuiApp::new(new_entries, app.current_dir.clone());
                app.apply_initial_expansion(args.expand_level);
                terminal.clear()?;
                app.rebuild_visible_list();
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(entry) = app.get_current_entry() {
                    break ExitAction::PrintPath(entry.data.path.clone());
                }
            }
            KeyCode::Right | KeyCode::Left | KeyCode::Enter => {
                if let Some(sel_idx) = app.list_state.selected() {
                    let entry_idx = app.filtered_indices[sel_idx];
                    let entry = &app.entries[entry_idx];
                    let name = entry
                        .data
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default();

                    if name == ".." {
                        app.go_up();
                    } else if entry.data.is_directory {
                        app.toggle_expansion();
                    } else {
                        let _ = open_file(&entry.data.path);
                    }
                }
            }
            KeyCode::Tab => {
                if let Some(sel_idx) = app.list_state.selected() {
                    let entry_idx = app.filtered_indices[sel_idx];
                    let entry = &app.entries[entry_idx];
                    let name = entry
                        .data
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default();

                    if name == ".." {
                        app.go_up();
                    } else if entry.data.is_directory {
                        app.enter_directory(entry_idx);
                    }

                    terminal.clear()?;
                    app.rebuild_visible_list();
                }
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(entry) = app.get_current_entry() {
                    let dir = if entry.data.is_directory {
                        entry.data.path.clone()
                    } else {
                        entry.data.path.parent().unwrap_or(&app.current_dir).to_path_buf()
                    };

                    // Esci temporaneamente dalla raw mode e dallo schermo alternativo
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                    terminal.show_cursor()?;
                    terminal.clear()?;

                    // Apri il terminale esterno
                    open_terminal(&dir)?;

                    // Rientra nella modalità TUI
                    enable_raw_mode()?;
                    execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                    terminal.clear()?;
                    app.rebuild_visible_list();
                }
            }
            KeyCode::Up => app.move_selection_up(),
            KeyCode::Down => app.move_selection_down(),
            KeyCode::Char('/') => app.start_search(),
            _ => {}
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    handle_exit_action(exit_action)
}

/// Open a terminal in the specified directory
#[inline]
fn open_terminal(dir: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").arg("/K").current_dir(dir).status()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("bash").current_dir(dir).status()?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").args(["-a", "Terminal"]).current_dir(dir).status()?;
    }
    Ok(())
}

/// Convert lscolors style to ratatui style
#[inline]
fn convert_ls_style(ls_style: LsStyle) -> Style {
    let mut style = Style::default();

    if let Some(fg) = ls_style.foreground {
        style = style.fg(match fg {
            LsColor::Black => Color::Black,
            LsColor::Red => Color::Red,
            LsColor::Green => Color::Green,
            LsColor::Yellow => Color::Yellow,
            LsColor::Blue => Color::Blue,
            LsColor::Magenta => Color::Magenta,
            LsColor::Cyan => Color::Cyan,
            LsColor::White => Color::White,
            LsColor::BrightBlack => Color::Gray,
            LsColor::BrightRed => Color::LightRed,
            LsColor::BrightGreen => Color::LightGreen,
            LsColor::BrightYellow => Color::LightYellow,
            LsColor::BrightBlue => Color::LightBlue,
            LsColor::BrightMagenta => Color::LightMagenta,
            LsColor::BrightCyan => Color::LightCyan,
            LsColor::BrightWhite => Color::White,
            LsColor::Fixed(n) => Color::Indexed(n),
            LsColor::RGB(r, g, b) => Color::Rgb(r, g, b),
        });
    }

    if ls_style.font_style.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if ls_style.font_style.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if ls_style.font_style.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }

    style
}

/// Handle what to do after exiting the TUI
fn handle_exit_action(action: ExitAction) -> anyhow::Result<()> {
    match action {
        ExitAction::PrintPath(path) => {
            println!("{}", canonicalize_path(path.as_path()).display());
        }
        ExitAction::None => {}
    }
    Ok(())
}

fn open_file(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("File does not exist: {}", path.display());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").args(["/C", "start", "", &path.display().to_string()]).spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}
