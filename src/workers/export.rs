use crate::app::Args;
use crate::common::tree::{Tree, TreeEntry};
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize)]
pub struct ExportNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub dir_count: Option<u64>,
    pub file_count: Option<u64>,
    pub permissions: String,
    pub children: Option<Vec<ExportNode>>,
}

pub enum OutputFormat {
    Json,
    Xml,
    Csv,
}

impl OutputFormat {
    pub fn from_str(s: &Option<String>) -> Option<Self> {
        match s.as_deref()?.to_lowercase().as_str() {
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}

pub fn export(args: &Args) -> Result<()> {
    if !args.path.is_dir() {
        anyhow::bail!("'{}' is not a directory.", args.path.display());
    }

    let start = std::time::Instant::now();
    let tree = Tree::prepare(args, true)?;

    let format = OutputFormat::from_str(&args.out).ok_or_else(|| {
        anyhow::anyhow!("Invalid format: {}", args.out.clone().unwrap_or_default())
    })?;

    let out_path = format!("export.{}", args.out.as_ref().unwrap());

    match format {
        OutputFormat::Csv => {
            let flat_nodes = build_export_flat_list(&tree, args)?;
            let mut wtr = csv::Writer::from_path(out_path)?;
            wtr.write_record([
                "path",
                "name",
                "is_dir",
                "size",
                "dir_count",
                "file_count",
                "permissions",
            ])?;
            for node in flat_nodes {
                wtr.write_record([
                    &node.path,
                    &node.name,
                    &node.is_dir.to_string(),
                    &node.size.map_or(String::new(), |s| s.to_string()),
                    &node.dir_count.map_or(String::new(), |d| d.to_string()),
                    &node.file_count.map_or(String::new(), |f| f.to_string()),
                    &node.permissions,
                ])?;
            }
            wtr.flush()?;
        }
        OutputFormat::Json | OutputFormat::Xml => {
            let export_root = build_export_tree(&tree, args);

            match format {
                OutputFormat::Json => {
                    fs::write(out_path, serde_json::to_string_pretty(&export_root)?)?
                }
                OutputFormat::Xml => fs::write(out_path, serde_xml_rs::to_string(&export_root)?)?,
                _ => {}
            }
        }
    }

    println!("Export completed in {:.2?}", start.elapsed());
    Ok(())
}

fn build_export_flat_list(tree: &Tree, args: &Args) -> Result<Vec<ExportNode>> {
    let default_info = TreeEntry::default();
    let canonical_root = fs::canonicalize(&args.path).unwrap_or(args.path.clone());
    let root_name = args.path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let mut flat_nodes = Vec::with_capacity(tree.entries.len());
    for (idx, entry) in tree.entries.iter().enumerate() {
        if args.dirs_only && !entry.file_type().is_some_and(|ft| ft.is_dir()) {
            continue;
        }

        let info = tree.tree_info.get(idx).unwrap_or(&default_info);
        let display_path = display_path(entry.path(), &canonical_root, &root_name);

        flat_nodes.push(ExportNode {
            name: entry.file_name().to_string_lossy().to_string(),
            path: display_path,
            is_dir: info.is_directory,
            size: info.size,
            dir_count: info.dirs,
            file_count: info.files,
            permissions: if args.permissions {
                info.permissions.clone().unwrap_or_default()
            } else {
                String::new()
            },
            children: None,
        });
    }

    Ok(flat_nodes)
}

fn build_export_tree(tree: &Tree, args: &Args) -> ExportNode {
    let root_path = fs::canonicalize(&args.path).unwrap_or(args.path.clone());
    let root_name = root_path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let mut children_map: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (idx, entry) in tree.entries.iter().enumerate() {
        if let Some(parent) = entry.path().parent() {
            children_map.entry(parent.to_path_buf()).or_default().push(idx);
        }
    }

    fn build_node(
        idx: usize,
        tree: &Tree,
        children_map: &HashMap<PathBuf, Vec<usize>>,
        root_path: &Path,
        root_name: &str,
        args: &Args,
    ) -> ExportNode {
        let entry = &tree.entries[idx];
        let info = &tree.tree_info[idx];
        let path = entry.path();

        let mut children = Vec::new();
        if let Some(child_indices) = children_map.get(path) {
            for &child_idx in child_indices {
                let child = build_node(child_idx, tree, children_map, root_path, root_name, args);
                if args.dirs_only && !child.is_dir {
                    continue;
                }
                children.push(child);
            }
        }

        ExportNode {
            name: entry.file_name().to_string_lossy().to_string(),
            path: display_path(path, root_path, root_name),
            is_dir: info.is_directory,
            size: info.size,
            dir_count: info.dirs,
            file_count: info.files,
            permissions: if args.permissions {
                info.permissions.clone().unwrap_or_default()
            } else {
                String::new()
            },
            children: if children.is_empty() { None } else { Some(children) },
        }
    }

    let mut root_children = Vec::new();
    if let Some(child_indices) = children_map.get(&root_path) {
        for &child_idx in child_indices {
            let child = build_node(child_idx, tree, &children_map, &root_path, &root_name, args);
            if args.dirs_only && !child.is_dir {
                continue;
            }
            root_children.push(child);
        }
    }

    let root_permissions = if args.permissions {
        fs::metadata(&root_path)
            .ok()
            .map(|metadata| crate::utils::dir::get_permission(Some(metadata)))
            .unwrap_or_default()
    } else {
        String::new()
    };

    let root_entries = tree.entries_at_depth(1);
    ExportNode {
        name: root_name.clone(),
        path: format!("./{root_name}"),
        is_dir: true,
        size: if args.size || args.info {
            Some(root_entries.iter().map(|(_, info)| info.size.unwrap_or(0)).sum())
        } else {
            None
        },
        dir_count: if args.info {
            Some(
                root_entries
                    .iter()
                    .filter(|(_, info)| info.is_directory)
                    .count() as u64,
            )
        } else {
            None
        },
        file_count: if args.info {
            Some(
                root_entries
                    .iter()
                    .filter(|(_, info)| !info.is_directory)
                    .count() as u64,
            )
        } else {
            None
        },
        permissions: root_permissions,
        children: if root_children.is_empty() { None } else { Some(root_children) },
    }
}

fn display_path(path: &Path, canonical_root: &Path, root_name: &str) -> String {
    if path == canonical_root {
        format!("./{root_name}")
    } else if let Ok(rel) = path.strip_prefix(canonical_root) {
        format!("./{root_name}/{}", rel.display())
    } else {
        path.display().to_string()
    }
}
