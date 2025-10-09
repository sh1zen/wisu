use crate::app::Args;
use crate::common::{icons, sort};
use crate::utils::dir;
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;

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
    pub max_depth: usize,
    depth_index: HashMap<usize, Vec<usize>>, // pre-index by depth
}

impl Tree {
    /// Builds the tree from DirEntry and Args
    fn build(entries: Vec<ignore::DirEntry>, args: &Args) -> Self {
        let mut infos: HashMap<std::path::PathBuf, TreeEntry> = HashMap::new();
        let mut max_depth = 0;

        // Root
        infos.insert(args.path.canonicalize().unwrap_or(args.path.clone()), TreeEntry::default());

        // First pass: gather info about files and directories
        for entry in &entries {
            let path = entry.path();
            let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());
            let size = if !is_dir { entry.metadata().map(|m| m.len()).unwrap_or(0) } else { 0 };

            infos.entry(path.to_path_buf()).or_insert_with(|| TreeEntry {
                size: if size > 0 { Some(size) } else { None },
                dirs: Some(0),
                files: Some(0),
                ..Default::default()
            });

            if !is_dir {
                let info = infos.get_mut(path).unwrap();
                info.files = Some(1);
                info.size = Some(size);
            }

            max_depth = max_depth.max(entry.depth());
        }

        // Propagation upward
        for entry in entries.iter().rev() {
            let path = entry.path();
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
            if let Some(parent) = path.parent() {
                let current = infos.get(path).cloned().unwrap_or_default();
                let parent_info = infos.entry(parent.to_path_buf()).or_default();

                parent_info.dirs = Some(parent_info.dirs.unwrap_or(0) + if is_dir { 1 } else { 0 });
                parent_info.files =
                    Some(parent_info.files.unwrap_or(0) + if !is_dir { 1 } else { 0 });
                parent_info.size = Some(parent_info.size.unwrap_or(0) + current.size.unwrap_or(0));
            }
        }

        // Filter entries according to args.files_only and args.files
        let mut filtered_entries = Vec::new();
        let mut files_count_in_dir: HashMap<std::path::PathBuf, usize> = HashMap::new();

        for entry in &entries {
            let path = entry.path();
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());

            // If args.files_only is true, skip directories
            if args.files_only && is_dir {
                continue;
            }

            if !is_dir {
                if let Some(max_files) = args.files {
                    let parent = path.parent().unwrap_or(path);
                    let count = files_count_in_dir.entry(parent.to_path_buf()).or_default();
                    if *count >= max_files {
                        if let Some(parent_info) = infos.get_mut(parent) {
                            parent_info.files = Some(parent_info.files.unwrap_or(0) + 1);
                            parent_info.size = Some(
                                parent_info.size.unwrap_or(0)
                                    + entry.metadata().map(|m| m.len()).unwrap_or(0),
                            );
                        }
                        continue;
                    } else {
                        *count += 1;
                    }
                }
            }

            filtered_entries.push(entry.clone());
        }

        // Build PrintTree and depth_index on filtered_entries
        let mut tree_info = Vec::with_capacity(filtered_entries.len());
        let mut depth_index: HashMap<usize, Vec<usize>> = HashMap::new();

        for (i, entry) in filtered_entries.iter().enumerate() {
            let path = entry.path();
            let original_depth = entry.depth();
            // If files_only, force depth to 0
            let depth = if args.files_only { 1 } else { original_depth };

            let is_last = !filtered_entries.iter().skip(i + 1).any(|e| {
                let e_depth = if args.files_only { 1 } else { e.depth() };
                e_depth == depth && e.path().parent() == path.parent()
            });

            let connector = if is_last { "└──" } else { "├──" };
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());
            let permissions = if args.permissions {
                Some(dir::get_permission(entry.metadata().ok()))
            } else {
                None
            };
            let icon = if args.icons {
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

            depth_index.entry(depth).or_default().push(i);
        }

        Tree { entries: filtered_entries, tree_info, max_depth, depth_index }
    }

    /// Prepares the tree from Args (scans files and directories)
    pub fn prepare(args: &Args) -> anyhow::Result<Self> {
        let mut builder = WalkBuilder::new(&args.path);
        builder.hidden(!args.all).git_ignore(args.gitignore);

        if let Some(level) = args.level {
            builder.max_depth(Some(level));
        }

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Scanning: {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
        );
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        let mut entries = Vec::new();
        for entry in builder.build().filter_map(Result::ok) {
            if entry.depth() == 0 {
                continue;
            }
            spinner.set_message(format!("{}", entry.path().display()));
            entries.push(entry);
        }
        spinner.finish_with_message("Completed ✅");

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Computing: {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
        );
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        if args.files_only {
            sort::sort_entries(&mut entries, &args.to_sort_options())
        } else {
            sort::sort_entries_hierarchically(&mut entries, &args.to_sort_options());
        }

        let tree = Self::build(entries, args);

        spinner.finish_with_message("Completed ✅");
        println!("\n");

        Ok(tree)
    }

    /// Returns all entries at a given depth along with their info
    pub fn entries_at_depth(&self, depth: usize) -> Vec<(&ignore::DirEntry, &TreeEntry)> {
        if let Some(indices) = self.depth_index.get(&depth) {
            indices.iter().map(|&i| (&self.entries[i], &self.tree_info[i])).collect()
        } else {
            Vec::new()
        }
    }
}
