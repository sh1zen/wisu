use crate::app::Args;
use crate::common::plugins::apply_filter;
use crate::common::{icons, sort};
use crate::utils::dir;
use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Component;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

/// Structure containing useful information for printing each entry
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: std::path::PathBuf,
    pub depth: usize,
    pub connector: String,
    pub size: Option<u64>,
    pub dirs: Option<u64>,
    pub files: Option<u64>,
    pub permissions: Option<String>,
    pub icon: Option<String>,
    pub is_directory: bool,
}

impl Default for TreeEntry {
    fn default() -> Self {
        TreeEntry {
            path: std::path::PathBuf::new(),
            depth: 0,
            connector: "└──".into(),
            size: None,
            dirs: None,
            files: None,
            permissions: None,
            icon: None,
            is_directory: false,
        }
    }
}

/// Tree of files and directories with information for printing
#[derive(Debug)]
pub struct Tree {
    pub entries: Vec<ignore::DirEntry>,
    pub tree_info: Vec<TreeEntry>,
    depth_index: HashMap<usize, Vec<usize>>,
}

/// Watch mode handle for filesystem monitoring
pub struct TreeWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<Result<Event, notify::Error>>,
}

impl TreeWatcher {
    /// Collect all pending changed paths (non-blocking)
    pub fn collect_changed_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        while let Ok(result) = self.receiver.try_recv() {
            if let Ok(event) = result {
                paths.extend(event.paths);
            }
        }
        paths
    }

    /// Wait for the next filesystem change (blocking)
    pub fn wait_for_change(&self) -> bool {
        self.receiver.recv().is_ok()
    }

    /// Wait for changes with timeout
    pub fn wait_for_change_timeout(&self, timeout: Duration) -> bool {
        self.receiver.recv_timeout(timeout).is_ok()
    }

    /// Drain all pending events (useful after rebuild)
    pub fn drain_events(&self) {
        while self.receiver.try_recv().is_ok() {}
    }
}

/// Helper function to check if a file passes the time filter
fn file_passes_time_filter(entry: &ignore::DirEntry, args: &Args) -> bool {
    let Some(ref time_filter) = args.time else {
        return true;
    };

    let Ok(metadata) = entry.metadata() else {
        return false;
    };

    let Ok(modified) = metadata.modified() else {
        return false;
    };

    let file_time: DateTime<Utc> = modified.into();
    time_filter.matches(file_time)
}

/// Helper function to check if a file/directory should be excluded
#[inline]
fn should_exclude(entry: &ignore::DirEntry, args: &Args) -> bool {
    let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
    // Only exclude files, not directories
    !is_dir && args.is_excluded(entry.path())
}

impl Tree {
    fn compare_entries_by_aggregated_size(
        &self,
        idx_a: usize,
        idx_b: usize,
        args: &Args,
    ) -> Ordering {
        let a = &self.tree_info[idx_a];
        let b = &self.tree_info[idx_b];

        let mut ord = Ordering::Equal;

        if args.dirs_first {
            ord = match (a.is_directory, b.is_directory) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => Ordering::Equal,
            };
        }

        if ord == Ordering::Equal && args.dotfiles_first {
            let a_dot =
                a.path.file_name().and_then(|s| s.to_str()).is_some_and(|n| n.starts_with('.'));
            let b_dot =
                b.path.file_name().and_then(|s| s.to_str()).is_some_and(|n| n.starts_with('.'));
            ord = match (a_dot, b_dot) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => Ordering::Equal,
            };
        }

        if ord == Ordering::Equal {
            ord = a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0));
        }

        if ord == Ordering::Equal {
            let a_name =
                a.path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let b_name =
                b.path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();

            ord = if args.case_sensitive {
                a_name.cmp(&b_name)
            } else {
                a_name.to_lowercase().cmp(&b_name.to_lowercase())
            };
        }

        if args.reverse { ord.reverse() } else { ord }
    }

    fn reorder_hierarchically_by_aggregated_size(&mut self, args: &Args) {
        let mut root_indices: Vec<usize> = self
            .tree_info
            .iter()
            .enumerate()
            .filter_map(|(i, info)| (info.depth == 1).then_some(i))
            .collect();

        let mut children_by_parent: HashMap<std::path::PathBuf, Vec<usize>> = HashMap::new();

        for (i, info) in self.tree_info.iter().enumerate() {
            if info.depth <= 1 {
                continue;
            }
            if let Some(parent) = info.path.parent() {
                children_by_parent.entry(parent.to_path_buf()).or_default().push(i);
            }
        }

        root_indices.sort_unstable_by(|&a, &b| self.compare_entries_by_aggregated_size(a, b, args));
        for children in children_by_parent.values_mut() {
            children.sort_unstable_by(|&a, &b| self.compare_entries_by_aggregated_size(a, b, args));
        }

        let mut ordered_indices = Vec::with_capacity(self.entries.len());

        fn collect(
            idx: usize,
            children_by_parent: &HashMap<std::path::PathBuf, Vec<usize>>,
            tree_info: &[TreeEntry],
            out: &mut Vec<usize>,
        ) {
            out.push(idx);
            if let Some(children) = children_by_parent.get(&tree_info[idx].path) {
                for &child in children {
                    collect(child, children_by_parent, tree_info, out);
                }
            }
        }

        for idx in root_indices {
            collect(idx, &children_by_parent, &self.tree_info, &mut ordered_indices);
        }

        let old_entries = std::mem::take(&mut self.entries);
        let old_info = std::mem::take(&mut self.tree_info);

        self.entries = ordered_indices.iter().map(|&i| old_entries[i].clone()).collect();
        self.tree_info = ordered_indices.iter().map(|&i| old_info[i].clone()).collect();

        let mut depth_index: HashMap<usize, Vec<usize>> = HashMap::new();
        for (new_i, info) in self.tree_info.iter().enumerate() {
            depth_index.entry(info.depth).or_default().push(new_i);
        }
        self.depth_index = depth_index;
    }

    /// Prune directories that have no file descendants (used after time filtering)
    fn prune_empty_dirs(mut tree: Tree) -> Tree {
        // Pre-allocate with estimated capacity
        let estimated_files = tree.tree_info.iter().filter(|i| !i.is_directory).count();
        let mut paths_with_files: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::with_capacity(estimated_files);

        // Single pass to mark all paths with files
        for info in &tree.tree_info {
            if !info.is_directory {
                let mut path = info.path.as_path();
                while let Some(parent) = path.parent() {
                    if !paths_with_files.insert(parent.to_path_buf()) {
                        break;
                    }
                    path = parent;
                }
            }
        }

        // Filter in-place where possible
        let mut write_idx = 0;
        for read_idx in 0..tree.tree_info.len() {
            if !tree.tree_info[read_idx].is_directory
                || paths_with_files.contains(&tree.tree_info[read_idx].path)
            {
                if write_idx != read_idx {
                    tree.entries[write_idx] = tree.entries[read_idx].clone();
                    tree.tree_info[write_idx] = tree.tree_info[read_idx].clone();
                }
                write_idx += 1;
            }
        }

        tree.entries.truncate(write_idx);
        tree.tree_info.truncate(write_idx);

        // Rebuild depth_index with known capacity
        let mut depth_index: HashMap<usize, Vec<usize>> = HashMap::new();
        for (new_i, info) in tree.tree_info.iter().enumerate() {
            depth_index.entry(info.depth).or_insert_with(Vec::new).push(new_i);
        }

        tree.depth_index = depth_index;
        tree
    }

    /// Builds the tree from DirEntry and Args
    fn build(entries: Vec<ignore::DirEntry>, args: &Args) -> Self {
        // Pre-allocate with capacity
        let capacity = entries.len() + 1;
        let mut infos: HashMap<std::path::PathBuf, TreeEntry> = HashMap::with_capacity(capacity);

        // Root
        let root_path = args.path.canonicalize().unwrap_or_else(|_| args.path.clone());
        infos.insert(root_path, TreeEntry::default());

        // First pass: gather info about files and directories
        for entry in &entries {
            let path = entry.path();
            let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());

            let info = infos.entry(path.to_path_buf()).or_insert_with(TreeEntry::default);

            info.is_directory = is_dir;

            if !is_dir {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                info.files = Some(1);
                info.size = Some(size);
                info.dirs = Some(0);
            } else {
                info.size.get_or_insert(0);
                info.dirs.get_or_insert(0);
                info.files.get_or_insert(0);
            }
        }

        // Propagation upward - single pass in reverse
        for entry in entries.iter().rev() {
            let path = entry.path();
            let Some(parent_path) = path.parent() else { continue };

            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());

            // Get values before borrowing mutably
            let (size, dirs, files) = {
                let current = infos.get(path).cloned().unwrap_or_default();
                (
                    current.size.unwrap_or(0),
                    if is_dir { current.dirs.unwrap_or(0) + 1 } else { 0 },
                    if is_dir { current.files.unwrap_or(0) } else { current.files.unwrap_or(1) },
                )
            };

            let parent_info = infos.entry(parent_path.to_path_buf()).or_default();
            parent_info.dirs = Some(parent_info.dirs.unwrap_or(0) + dirs);
            parent_info.files = Some(parent_info.files.unwrap_or(0) + files);
            parent_info.size = Some(parent_info.size.unwrap_or(0) + size);
        }

        // Filter entries according to args.files_only and args.files
        let max_files = args.files;
        let files_only = args.files_only;
        let dirs_only = args.dirs_only;
        let max_level = args.level;
        let mut filtered_entries = Vec::with_capacity(entries.len());
        let mut files_count_in_dir: HashMap<std::path::PathBuf, usize> = HashMap::new();

        for entry in entries {
            let path = entry.path();
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
            let depth = entry.depth();

            if let Some(max) = max_level {
                if depth > max {
                    continue;
                }
            }

            if files_only && is_dir {
                continue;
            }

            if dirs_only && !is_dir {
                continue;
            }

            if !is_dir {
                if let Some(max) = max_files {
                    let parent = path.parent().unwrap_or(path);
                    let count = files_count_in_dir.entry(parent.to_path_buf()).or_insert(0);

                    if *count >= max {
                        if let Some(parent_info) = infos.get_mut(parent) {
                            parent_info.files = Some(parent_info.files.unwrap_or(0) + 1);
                            parent_info.size = Some(
                                parent_info.size.unwrap_or(0)
                                    + entry.metadata().map(|m| m.len()).unwrap_or(0),
                            );
                        }
                        continue;
                    }
                    *count += 1;
                }
            }

            filtered_entries.push(entry);
        }

        // Build tree_info and depth_index
        let len = filtered_entries.len();
        let mut tree_info = Vec::with_capacity(len);
        let mut depth_index: HashMap<usize, Vec<usize>> = HashMap::new();

        let show_permissions = args.permissions;
        let show_icons = args.icons;

        for (i, entry) in filtered_entries.iter().enumerate() {
            let path = entry.path();
            let original_depth = entry.depth();
            let depth = if files_only { 1 } else { original_depth };

            // Optimized is_last check
            let is_last = filtered_entries[i + 1..].iter().all(|e| {
                let e_depth = if files_only { 1 } else { e.depth() };
                e_depth != depth || e.path().parent() != path.parent()
            });

            let connector = if is_last { "└──" } else { "├──" };
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());

            let permissions = if show_permissions {
                Some(dir::get_permission(entry.metadata().ok()))
            } else {
                None
            };

            let icon = if show_icons {
                Some(format!("{} ", icons::get_icon_for_path(path, is_dir)))
            } else {
                None
            };

            let info = infos.get(path).cloned().unwrap_or_default();

            tree_info.push(TreeEntry {
                path: path.to_path_buf(),
                depth,
                connector: connector.to_string(),
                size: info.size,
                dirs: info.dirs,
                files: info.files,
                permissions,
                icon,
                is_directory: is_dir,
            });

            depth_index.entry(depth).or_insert_with(Vec::new).push(i);
        }

        let mut tree = Tree { entries: filtered_entries, tree_info, depth_index };

        if !args.files_only && matches!(args.sort, crate::app::SortType::Size) {
            tree.reorder_hierarchically_by_aggregated_size(args);
        }

        tree
    }

    /// Creates a filesystem watcher for the given path
    pub fn create_watcher(args: &Args) -> anyhow::Result<TreeWatcher> {
        let (tx, rx) = channel();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default().with_poll_interval(Duration::from_secs(1)),
        )?;

        let watch_mode = if args.level.is_some() {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };

        watcher.watch(&args.path, watch_mode)?;

        Ok(TreeWatcher { _watcher: watcher, receiver: rx })
    }

    /// Prepares the tree from Args (scans files and directories)
    pub fn prepare(args: &Args, show_progress: bool) -> anyhow::Result<Self> {
        let mut builder = WalkBuilder::new(&args.path);
        builder.hidden(!args.all).git_ignore(args.gitignore);

        let spinner = if show_progress {
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
            );
            spinner.set_message("Scanning:".to_string());
            spinner.enable_steady_tick(Duration::from_millis(80));
            spinner
        } else {
            ProgressBar::hidden()
        };

        let mut entries = Vec::new();
        let has_time_filter = args.time.is_some();
        let has_exclude_filter = args.exclude.is_some();
        let excluded_dirs = args.get_excluded_directories();
        let has_excluded_dirs = !excluded_dirs.is_empty();
        for entry in builder.build().filter_map(Result::ok) {
            if entry.depth() == 0 {
                continue;
            }

            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());

            if has_excluded_dirs {
                let rel_path = entry.path().strip_prefix(&args.path).unwrap_or(entry.path());
                let in_excluded_dir = rel_path.components().any(|c| match c {
                    Component::Normal(name) => {
                        excluded_dirs.contains(&name.to_string_lossy().to_lowercase())
                    }
                    _ => false,
                });

                if in_excluded_dir {
                    continue;
                }
            }

            // Apply exclude filter (only to files)
            if has_exclude_filter && should_exclude(&entry, args) {
                continue;
            }

            // Apply time filter only to files (dirs added unconditionally, pruned later)
            if has_time_filter && !is_dir && !file_passes_time_filter(&entry, args) {
                continue;
            }

            if show_progress {
                spinner.set_message(format!("Scanning: {}", entry.path().display()));
            }
            entries.push(entry);
        }

        let spinner = if show_progress {
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
            );
            spinner.set_message("Computing:".to_string());
            spinner.enable_steady_tick(Duration::from_millis(80));
            spinner
        } else {
            ProgressBar::hidden()
        };

        if args.files_only {
            sort::sort_entries(&mut entries, &args.to_sort_options())
        } else {
            sort::sort_entries_hierarchically(&mut entries, &args.to_sort_options());
        }

        let tree = Self::build(entries, args);

        // Prune empty directories if time filter or exclude filter is active
        let tree =
            if has_time_filter || has_exclude_filter { Self::prune_empty_dirs(tree) } else { tree };

        Ok(apply_filter("tree_entries", tree))
    }

    /// Prepares the tree with watch mode support
    pub fn prepare_with_watch(
        args: &Args,
        show_progress: bool,
    ) -> anyhow::Result<(Self, Option<TreeWatcher>)> {
        let tree = Self::prepare(args, show_progress)?;

        let watcher = if args.watch { Some(Self::create_watcher(args)?) } else { None };

        Ok((tree, watcher))
    }

    /// Returns all entries at a given depth along with their info
    pub fn entries_at_depth(&self, depth: usize) -> Vec<(&ignore::DirEntry, &TreeEntry)> {
        self.depth_index
            .get(&depth)
            .map(|indices| {
                indices.iter().map(|&i| (&self.entries[i], &self.tree_info[i])).collect()
            })
            .unwrap_or_default()
    }
}
