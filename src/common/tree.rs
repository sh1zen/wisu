use crate::app::Args;
use crate::common::entry::{CachedMetadata, PreparedEntry};
use crate::common::plugins::apply_filter;
use crate::common::{icons, sort};
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
    pub is_executable: bool,
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
            is_executable: false,
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

#[cfg(test)]
mod tests {
    use super::Tree;
    use crate::app::Args;
    use clap::Parser;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn prepare_skips_aggregates_when_not_requested() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("a.txt"), "abc").unwrap();
        fs::create_dir(temp_dir.path().join("dir1")).unwrap();
        fs::write(temp_dir.path().join("dir1").join("b.txt"), "def").unwrap();

        let mut args = Args::parse_from(["wisu", temp_dir.path().to_str().unwrap()]);
        args.stats = false;
        let tree = Tree::prepare(&args, false).unwrap();

        assert!(tree.tree_info.iter().all(|info| info.size.is_none()));
        assert!(tree.tree_info.iter().all(|info| info.dirs.is_none()));
        assert!(tree.tree_info.iter().all(|info| info.files.is_none()));
        assert!(tree.tree_info.iter().all(|info| info.permissions.is_none()));
    }

    #[test]
    fn prepare_keeps_sizes_when_stats_are_enabled() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("a.txt"), "abc").unwrap();

        let args = Args::parse_from(["wisu", temp_dir.path().to_str().unwrap()]);
        let tree = Tree::prepare(&args, false).unwrap();

        assert!(tree.tree_info.iter().any(|info| info.size.is_some()));
    }

    #[test]
    fn prepare_marks_last_siblings_correctly() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join("a")).unwrap();
        fs::create_dir(temp_dir.path().join("b")).unwrap();
        fs::write(temp_dir.path().join("a").join("child.txt"), "x").unwrap();

        let args = Args::parse_from(["wisu", temp_dir.path().to_str().unwrap()]);
        let tree = Tree::prepare(&args, false).unwrap();

        let connectors: Vec<_> = tree
            .tree_info
            .iter()
            .map(|info| {
                (
                    info.path.file_name().unwrap().to_string_lossy().to_string(),
                    info.connector.clone(),
                )
            })
            .collect();

        assert_eq!(
            connectors,
            vec![
                ("a".to_string(), "├──".to_string()),
                ("child.txt".to_string(), "└──".to_string()),
                ("b".to_string(), "└──".to_string()),
            ]
        );
    }
}

/// Helper function to check if a file passes the time filter
fn file_passes_time_filter(metadata: &CachedMetadata, args: &Args) -> bool {
    let Some(ref time_filter) = args.time else {
        return true;
    };

    let Some(modified) = metadata.modified else {
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
    fn compute_is_last_flags(entries: &[PreparedEntry], files_only: bool) -> Vec<bool> {
        let mut is_last = vec![true; entries.len()];

        if files_only {
            let mut last_index_by_parent: HashMap<std::path::PathBuf, usize> = HashMap::new();

            for (idx, entry) in entries.iter().enumerate() {
                let parent = entry
                    .entry
                    .path()
                    .parent()
                    .unwrap_or(entry.entry.path())
                    .to_path_buf();

                if let Some(previous_idx) = last_index_by_parent.insert(parent, idx) {
                    is_last[previous_idx] = false;
                }
            }

            return is_last;
        }

        let mut ancestors_at_depth: Vec<usize> = Vec::new();
        let mut last_child_by_parent: Vec<Option<usize>> = vec![None; entries.len()];
        let mut last_root_index: Option<usize> = None;

        for (idx, entry) in entries.iter().enumerate() {
            let depth = entry.entry.depth();
            let parent_depth = depth.saturating_sub(1);
            ancestors_at_depth.truncate(parent_depth);

            if let Some(&parent_idx) = ancestors_at_depth.last() {
                if let Some(previous_idx) = last_child_by_parent[parent_idx].replace(idx) {
                    is_last[previous_idx] = false;
                }
            } else if let Some(previous_idx) = last_root_index.replace(idx) {
                is_last[previous_idx] = false;
            }

            ancestors_at_depth.push(idx);
        }

        is_last
    }

    fn load_cached_metadata(entry: &ignore::DirEntry, args: &Args) -> CachedMetadata {
        if !args.needs_filesystem_metadata() {
            return CachedMetadata::default();
        }

        let needs_times = args.needs_time_metadata();

        match entry.metadata() {
            Ok(metadata) => {
                #[cfg(unix)]
                use std::os::unix::fs::PermissionsExt;

                CachedMetadata {
                    size: if args.needs_aggregated_metadata() && !metadata.is_dir() {
                        metadata.len()
                    } else {
                        0
                    },
                    accessed: needs_times.then(|| metadata.accessed().ok()).flatten(),
                    created: needs_times.then(|| metadata.created().ok()).flatten(),
                    modified: needs_times.then(|| metadata.modified().ok()).flatten(),
                    #[cfg(unix)]
                    mode: (args.needs_permissions_metadata() || args.needs_executable_metadata())
                        .then(|| metadata.permissions().mode()),
                }
            }
            Err(_) => CachedMetadata::default(),
        }
    }

    #[cfg(unix)]
    fn is_entry_executable(entry: &ignore::DirEntry, metadata: &CachedMetadata) -> bool {
        metadata.mode.is_some_and(|mode| mode & 0o111 != 0)
            && !entry.file_type().is_some_and(|ft| ft.is_dir())
    }

    #[cfg(windows)]
    fn is_entry_executable(entry: &ignore::DirEntry, _: &CachedMetadata) -> bool {
        entry.path()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "exe" | "bat" | "cmd"))
            .unwrap_or(false)
    }

    fn cached_permissions_string(
        entry: &ignore::DirEntry,
        metadata: &CachedMetadata,
    ) -> Option<String> {
        #[cfg(unix)]
        {
            metadata.mode.map(|mode| {
                let ft_char = if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                    'd'
                } else {
                    '-'
                };
                format!("{ft_char}{} ", crate::utils::format::format_permissions(mode))
            })
        }

        #[cfg(not(unix))]
        {
            let _ = entry;
            let _ = metadata;
            Some("---------- ".to_string())
        }
    }

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
        let mut root_indices = Vec::new();
        let mut children_by_parent: Vec<Vec<usize>> =
            (0..self.tree_info.len()).map(|_| Vec::new()).collect();
        let mut ancestors_at_depth: Vec<usize> = Vec::new();

        for (idx, info) in self.tree_info.iter().enumerate() {
            let parent_depth = info.depth.saturating_sub(1);
            ancestors_at_depth.truncate(parent_depth);

            if let Some(&parent_idx) = ancestors_at_depth.last() {
                children_by_parent[parent_idx].push(idx);
            } else {
                root_indices.push(idx);
            }

            ancestors_at_depth.push(idx);
        }

        root_indices.sort_unstable_by(|&a, &b| self.compare_entries_by_aggregated_size(a, b, args));
        for children in &mut children_by_parent {
            children.sort_unstable_by(|&a, &b| self.compare_entries_by_aggregated_size(a, b, args));
        }

        let mut ordered_indices = Vec::with_capacity(self.entries.len());
        let mut stack: Vec<usize> = root_indices.iter().rev().copied().collect();
        while let Some(idx) = stack.pop() {
            ordered_indices.push(idx);
            for &child_idx in children_by_parent[idx].iter().rev() {
                stack.push(child_idx);
            }
        }

        let mut old_entries: Vec<Option<ignore::DirEntry>> =
            std::mem::take(&mut self.entries).into_iter().map(Some).collect();
        let mut old_info: Vec<Option<TreeEntry>> =
            std::mem::take(&mut self.tree_info).into_iter().map(Some).collect();

        self.entries = ordered_indices
            .iter()
            .map(|&i| old_entries[i].take().expect("ordered indices must be unique"))
            .collect();
        self.tree_info = ordered_indices
            .iter()
            .map(|&i| old_info[i].take().expect("ordered indices must be unique"))
            .collect();

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

    /// Builds the tree from cached scan entries and Args
    fn build(entries: Vec<PreparedEntry>, args: &Args) -> Self {
        let needs_aggregates = args.needs_aggregated_metadata();
        let mut infos = if needs_aggregates {
            let capacity = entries.len() + 1;
            let mut infos = HashMap::with_capacity(capacity);
            let root_path = args.path.canonicalize().unwrap_or_else(|_| args.path.clone());
            infos.insert(root_path, TreeEntry::default());
            Some(infos)
        } else {
            None
        };

        if let Some(infos) = infos.as_mut() {
            for entry in &entries {
                let path = entry.entry.path();
                let is_dir = entry.entry.file_type().is_some_and(|ft| ft.is_dir());

                let info = infos.entry(path.to_path_buf()).or_insert_with(TreeEntry::default);
                info.is_directory = is_dir;

                if !is_dir {
                    info.files = Some(1);
                    info.size = Some(entry.metadata.size);
                    info.dirs = Some(0);
                } else {
                    info.size.get_or_insert(0);
                    info.dirs.get_or_insert(0);
                    info.files.get_or_insert(0);
                }
            }

            for entry in entries.iter().rev() {
                let path = entry.entry.path();
                let Some(parent_path) = path.parent() else { continue };
                let is_dir = entry.entry.file_type().is_some_and(|ft| ft.is_dir());

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
        }

        // Filter entries according to args.files_only and args.files
        let max_files = args.files;
        let files_only = args.files_only;
        let dirs_only = args.dirs_only;
        let max_level = args.level;
        let mut filtered_entries = Vec::with_capacity(entries.len());
        let mut files_count_in_dir: HashMap<std::path::PathBuf, usize> = HashMap::new();

        for entry in entries {
            let path = entry.entry.path();
            let is_dir = entry.entry.file_type().is_some_and(|ft| ft.is_dir());
            let depth = entry.entry.depth();

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
                        if let Some(parent_info) =
                            infos.as_mut().and_then(|infos| infos.get_mut(parent))
                        {
                            parent_info.files = Some(parent_info.files.unwrap_or(0) + 1);
                            parent_info.size = Some(
                                parent_info.size.unwrap_or(0)
                                    + entry.metadata.size,
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
        let is_last_flags = Self::compute_is_last_flags(&filtered_entries, files_only);

        let show_permissions = args.permissions;
        let show_icons = args.icons;

        for (i, entry) in filtered_entries.iter().enumerate() {
            let path = entry.entry.path();
            let original_depth = entry.entry.depth();
            let depth = if files_only { 1 } else { original_depth };
            let is_last = is_last_flags[i];

            let connector = if is_last { "└──" } else { "├──" };
            let is_dir = entry.entry.file_type().is_some_and(|ft| ft.is_dir());
            let is_executable = Self::is_entry_executable(&entry.entry, &entry.metadata);

            let permissions = if show_permissions {
                Self::cached_permissions_string(&entry.entry, &entry.metadata)
            } else {
                None
            };

            let icon = if show_icons {
                Some(format!("{} ", icons::get_icon_for_path(path, is_dir)))
            } else {
                None
            };

            let info =
                infos.as_ref().and_then(|infos| infos.get(path)).cloned().unwrap_or_default();

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
                is_executable,
            });

            depth_index.entry(depth).or_insert_with(Vec::new).push(i);
        }

        let mut tree = Tree {
            entries: filtered_entries.into_iter().map(|entry| entry.entry).collect(),
            tree_info,
            depth_index,
        };

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

            let cached_metadata = Self::load_cached_metadata(&entry, args);

            // Apply time filter only to files (dirs added unconditionally, pruned later)
            if has_time_filter && !is_dir && !file_passes_time_filter(&cached_metadata, args) {
                continue;
            }

            if show_progress {
                spinner.set_message(format!("Scanning: {}", entry.path().display()));
            }
            entries.push(PreparedEntry { entry, metadata: cached_metadata });
        }

        let _spinner = if show_progress {
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
