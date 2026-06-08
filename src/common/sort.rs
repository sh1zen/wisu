//! Provides OS-agnostic sorting functionality for directory entries.
//!
//! This module implements various sorting strategies for file and directory entries,
//! ensuring consistent behavior across all supported platforms (Windows, macOS, Linux).

use crate::common::entry::PreparedEntry;
use std::cmp::Ordering;
use std::path::Path;
use std::time::SystemTime;

/// Defines the available sorting strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortType {
    Name,
    Size,
    Accessed,
    Created,
    Modified,
    Extension,
}

impl Default for SortType {
    fn default() -> Self {
        Self::Name
    }
}

/// Configuration options for sorting directory entries.
#[derive(Debug, Clone)]
pub struct SortOptions {
    pub sort_type: SortType,
    pub directories_first: bool,
    pub case_sensitive: bool,
    pub natural_sort: bool,
    pub reverse: bool,
    pub dotfiles_first: bool,
}

impl Default for SortOptions {
    fn default() -> Self {
        Self {
            sort_type: SortType::default(),
            directories_first: false,
            case_sensitive: false,
            natural_sort: false,
            reverse: false,
            dotfiles_first: false,
        }
    }
}

/// Cached metadata for efficient sorting without repeated syscalls.
#[derive(Debug, Clone)]
struct EntryCache {
    is_dir: bool,
    is_dotfile: bool,
    size: u64,
    accessed: Option<SystemTime>,
    created: Option<SystemTime>,
    modified: Option<SystemTime>,
    extension: Option<String>, 
    cached_name: String,
}

impl EntryCache {
    fn new(entry: &PreparedEntry, options: &SortOptions) -> Self {
        let dir_entry = &entry.entry;
        let file_name = dir_entry.file_name();
        let file_name_str = file_name.to_string_lossy().to_string();
        let is_dotfile = file_name_str.starts_with('.');
        let is_dir = dir_entry.file_type().is_some_and(|ft| ft.is_dir());

        let extension = if options.sort_type == SortType::Extension {
            Path::new(&file_name_str).extension().and_then(|e| e.to_str()).map(|s| s.to_string())
        } else {
            None
        };

        Self {
            is_dir,
            is_dotfile,
            size: if is_dir { 0 } else { entry.metadata.size },
            accessed: entry.metadata.accessed,
            created: entry.metadata.created,
            modified: entry.metadata.modified,
            extension,
            cached_name: file_name_str,
        }
    }
}

/// Sorts a slice of directory entries according to the given options.
pub fn sort_entries(entries: &mut [PreparedEntry], options: &SortOptions) {
    if entries.len() <= 1 {
        return;
    }

    let cache: Vec<EntryCache> = entries.iter().map(|e| EntryCache::new(e, options)).collect();
    let mut indices: Vec<usize> = (0..entries.len()).collect();

    indices.sort_unstable_by(|&idx_a, &idx_b| {
        let cmp = compare_entries_cached(&cache[idx_a], &cache[idx_b], options);
        if options.reverse { cmp.reverse() } else { cmp }
    });

    let mut visited = vec![false; entries.len()];
    for start in 0..entries.len() {
        if visited[start] {
            continue;
        }
        let mut current = start;
        let mut next = indices[current];
        while next != start {
            entries.swap(current, next);
            visited[current] = true;
            current = next;
            next = indices[current];
        }
        visited[current] = true;
    }
}

/// Sorts directory entries hierarchically, preserving tree structure.
pub fn sort_entries_hierarchically(entries: &mut Vec<PreparedEntry>, options: &SortOptions) {
    if entries.len() <= 1 {
        return;
    }

    let cache: Vec<EntryCache> = entries.iter().map(|e| EntryCache::new(e, options)).collect();
    let (mut root_indices, mut children_by_parent) = build_index_tree(entries.iter().map(|e| e.entry.depth()));

    sort_index_group(&mut root_indices, &cache, options);
    for children in &mut children_by_parent {
        sort_index_group(children, &cache, options);
    }

    let ordered_indices = collect_ordered_indices(&root_indices, &children_by_parent, entries.len());
    reorder_vec_by_indices(entries, ordered_indices);
}

#[inline]
fn sort_index_group(indices: &mut [usize], cache: &[EntryCache], options: &SortOptions) {
    indices.sort_unstable_by(|&idx_a, &idx_b| {
        let cmp = compare_entries_cached(&cache[idx_a], &cache[idx_b], options);
        if options.reverse { cmp.reverse() } else { cmp }
    });
}

fn build_index_tree<I>(depths: I) -> (Vec<usize>, Vec<Vec<usize>>)
where
    I: IntoIterator<Item = usize>,
{
    let depths: Vec<usize> = depths.into_iter().collect();
    let mut root_indices = Vec::new();
    let mut children_by_parent: Vec<Vec<usize>> = (0..depths.len()).map(|_| Vec::new()).collect();
    let mut ancestors_at_depth: Vec<usize> = Vec::new();

    for (idx, depth) in depths.into_iter().enumerate() {
        let parent_depth = depth.saturating_sub(1);
        ancestors_at_depth.truncate(parent_depth);

        if let Some(&parent_idx) = ancestors_at_depth.last() {
            children_by_parent[parent_idx].push(idx);
        } else {
            root_indices.push(idx);
        }

        ancestors_at_depth.push(idx);
    }

    (root_indices, children_by_parent)
}

fn collect_ordered_indices(
    root_indices: &[usize],
    children_by_parent: &[Vec<usize>],
    len: usize,
) -> Vec<usize> {
    let mut ordered_indices = Vec::with_capacity(len);
    let mut stack: Vec<usize> = root_indices.iter().rev().copied().collect();

    while let Some(idx) = stack.pop() {
        ordered_indices.push(idx);
        for &child_idx in children_by_parent[idx].iter().rev() {
            stack.push(child_idx);
        }
    }

    ordered_indices
}

fn reorder_vec_by_indices<T>(values: &mut Vec<T>, ordered_indices: Vec<usize>) {
    let mut old_values: Vec<Option<T>> = std::mem::take(values).into_iter().map(Some).collect();
    values.reserve(old_values.len());
    values.extend(ordered_indices.into_iter().map(|idx| {
        old_values[idx].take().expect("ordered indices must be unique and in bounds")
    }));
}

#[inline]
fn compare_entries_cached(
    cache_a: &EntryCache,
    cache_b: &EntryCache,
    options: &SortOptions,
) -> Ordering {
    if let Some(order) = compare_file_categories(cache_a, cache_b, options) {
        return order;
    }

    match options.sort_type {
        SortType::Name => compare_by_cached_name(
            &cache_a.cached_name,
            &cache_b.cached_name,
            options.natural_sort,
            options.case_sensitive,
        ),
        SortType::Size => cache_a.size.cmp(&cache_b.size),
        SortType::Accessed => compare_by_time(&cache_a.accessed, &cache_b.accessed),
        SortType::Created => compare_by_time(&cache_a.created, &cache_b.created),
        SortType::Modified => compare_by_time(&cache_a.modified, &cache_b.modified),
        SortType::Extension => {
            let ext_a = cache_a.extension.as_deref().unwrap_or("");
            let ext_b = cache_b.extension.as_deref().unwrap_or("");
            let ext_cmp = if options.case_sensitive {
                ext_a.cmp(ext_b)
            } else {
                ext_a.to_lowercase().cmp(&ext_b.to_lowercase())
            };
            if ext_cmp == Ordering::Equal {
                compare_by_cached_name(
                    &cache_a.cached_name,
                    &cache_b.cached_name,
                    options.natural_sort,
                    options.case_sensitive,
                )
            } else {
                ext_cmp
            }
        }
    }
}

#[inline]
fn compare_file_categories(
    cache_a: &EntryCache,
    cache_b: &EntryCache,
    options: &SortOptions,
) -> Option<Ordering> {
    if options.dotfiles_first {
        fn priority(is_dotfile: bool, is_dir: bool) -> u8 {
            match (is_dotfile, is_dir) {
                (true, true) => 0,   // Dotfile + Directory
                (false, true) => 1,  // Directory
                (true, false) => 2,  // Dotfile
                (false, false) => 3, // Regular file
            }
        }

        let priority_a = priority(cache_a.is_dotfile, cache_a.is_dir);
        let priority_b = priority(cache_b.is_dotfile, cache_b.is_dir);

        match priority_a.cmp(&priority_b) {
            Ordering::Equal => None,
            ord => Some(ord),
        }
    } else if options.directories_first {
        match cache_a.is_dir.cmp(&cache_b.is_dir).reverse() {
            Ordering::Equal => None,
            ord => Some(ord),
        }
    } else {
        None
    }
}

#[inline]
fn compare_by_cached_name(
    name_a: &str,
    name_b: &str,
    natural: bool,
    case_sensitive: bool,
) -> Ordering {
    let a_str = if case_sensitive || natural { name_a } else { &name_a.to_lowercase() };
    let b_str = if case_sensitive || natural { name_b } else { &name_b.to_lowercase() };

    if natural { natord::compare(a_str, b_str) } else { a_str.cmp(b_str) }
}

#[inline]
fn compare_by_time(time_a: &Option<SystemTime>, time_b: &Option<SystemTime>) -> Ordering {
    match (time_a, time_b) {
        (Some(a), Some(b)) => b.cmp(a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::entry::{CachedMetadata, PreparedEntry};
    use ignore::WalkBuilder;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    fn collect_entries_from_temp(names: &[&str]) -> Vec<PreparedEntry> {
        let dir = tempdir().unwrap();
        for name in names {
            let path = dir.path().join(name);
            if name.ends_with('/') {
                fs::create_dir_all(&path).unwrap();
            } else {
                File::create(&path).unwrap().write_all(b"test").unwrap();
            }
        }

        // Use WalkBuilder and configure it to NOT skip hidden files
        WalkBuilder::new(dir.path())
            .hidden(false) // <- include hidden files
            .build()
            .filter_map(Result::ok)
            .filter(|e| e.depth() == 1) // Only immediate children
            .map(|entry| PreparedEntry { entry, metadata: CachedMetadata::default() })
            .collect()
    }

    #[test]
    fn test_sort_by_name_case_insensitive() {
        let mut entries = collect_entries_from_temp(&["banana", "Apple"]);
        let mut options = SortOptions::default();
        options.case_sensitive = false;

        sort_entries(&mut entries, &options);
        let names: Vec<_> =
            entries.iter().map(|e| e.entry.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["Apple", "banana"]);
    }

    #[test]
    fn test_sort_by_name_case_sensitive() {
        let mut entries = collect_entries_from_temp(&["banana", "Apple"]);
        let mut options = SortOptions::default();
        options.case_sensitive = true;

        sort_entries(&mut entries, &options);
        let names: Vec<_> =
            entries.iter().map(|e| e.entry.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["Apple", "banana"]);
    }

    #[test]
    fn test_sort_by_extension() {
        let mut entries = collect_entries_from_temp(&["a.t", "b.b", "c.T"]);
        let mut options = SortOptions::default();
        options.sort_type = SortType::Extension;

        sort_entries(&mut entries, &options);
        let names: Vec<_> =
            entries.iter().map(|e| e.entry.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["b.b", "a.t", "c.T"]);
    }

    #[test]
    fn test_sort_reverse() {
        let mut entries = collect_entries_from_temp(&["a", "b", "c"]);
        let mut options = SortOptions::default();
        options.reverse = true;

        sort_entries(&mut entries, &options);
        let names: Vec<_> =
            entries.iter().map(|e| e.entry.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["c", "b", "a"]);
    }

    #[test]
    fn test_dotfiles_first() {
        let mut entries = collect_entries_from_temp(&[".hidden", "visible"]);

        let mut options = SortOptions::default();
        options.dotfiles_first = true;

        sort_entries(&mut entries, &options);

        let names: Vec<_> =
            entries.iter().map(|e| e.entry.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec![".hidden", "visible"]);
    }

    #[test]
    fn test_directories_first() {
        let mut entries = collect_entries_from_temp(&["dir/", "file.txt"]);
        let mut options = SortOptions::default();
        options.directories_first = true;

        sort_entries(&mut entries, &options);
        let names: Vec<_> =
            entries.iter().map(|e| e.entry.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["dir", "file.txt"]);
    }

    #[test]
    fn test_hierarchical_sort_reorders_only_siblings() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("b_dir")).unwrap();
        fs::create_dir_all(dir.path().join("a_dir")).unwrap();
        File::create(dir.path().join("b_dir").join("z.txt")).unwrap();
        File::create(dir.path().join("a_dir").join("a.txt")).unwrap();

        let mut entries: Vec<_> = WalkBuilder::new(dir.path())
            .hidden(false)
            .build()
            .filter_map(Result::ok)
            .filter(|e| e.depth() > 0)
            .map(|entry| PreparedEntry { entry, metadata: CachedMetadata::default() })
            .collect();

        let options = SortOptions::default();
        sort_entries_hierarchically(&mut entries, &options);

        let ordered: Vec<_> = entries
            .iter()
            .map(|e| {
                (
                    e.entry.depth(),
                    e.entry.file_name().to_string_lossy().to_string(),
                )
            })
            .collect();

        assert_eq!(
            ordered,
            vec![
                (1, "a_dir".to_string()),
                (2, "a.txt".to_string()),
                (1, "b_dir".to_string()),
                (2, "z.txt".to_string()),
            ]
        );
    }

    #[test]
    fn test_sort_options_default() {
        let options = SortOptions::default();
        assert_eq!(options.sort_type, SortType::Name);
        assert!(!options.case_sensitive);
        assert!(!options.natural_sort);
        assert!(!options.reverse);
        assert!(!options.dotfiles_first);
        assert!(!options.directories_first);
    }
}
