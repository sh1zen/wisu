use crate::app::Args;
use crate::common::tree::{TreeEntry, Tree};
use crate::utils::dir::get_permission;
use anyhow::Result;
use std::fs;

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

    // ───────────── Data Preparation ─────────────
    let tree = Tree::prepare(args)?;

    let format = OutputFormat::from_str(&args.out).ok_or_else(|| {
        anyhow::anyhow!("Invalid format: {}", args.out.clone().unwrap_or_default())
    })?;
    let out_path = args.out.as_ref().unwrap();

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

/// Exports the tree as a flat list
fn build_export_flat_list(tree: &Tree, args: &Args) -> Result<Vec<ExportNode>> {
    let default_info = TreeEntry::default();
    let canonical_root = fs::canonicalize(&args.path).unwrap_or(args.path.clone());

    let mut flat_nodes = Vec::new();
    for (idx, entry) in tree.entries.iter().enumerate() {
        if args.dirs_only && !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }

        let c_info = tree.tree_info.get(idx).unwrap_or(&default_info);

        let permissions =
            if args.permissions { get_permission(entry.metadata().ok()) } else { String::new() };

        let display_path = if entry.path() == canonical_root {
            format!("./{}", args.path.file_name().unwrap_or_default().to_string_lossy())
        } else if let Ok(rel) = entry.path().strip_prefix(&canonical_root) {
            format!(
                "./{}/{}",
                args.path.file_name().unwrap_or_default().to_string_lossy(),
                rel.display()
            )
        } else {
            entry.path().display().to_string()
        };

        flat_nodes.push(ExportNode {
            name: entry.file_name().to_string_lossy().to_string(),
            path: display_path,
            is_dir: entry.file_type().map(|ft| ft.is_dir()).unwrap_or(true),
            size: c_info.size,
            dir_count: c_info.dirs,
            file_count: c_info.files,
            permissions,
            children: None,
        });
    }

    Ok(flat_nodes)
}

/// Exports the tree as a hierarchical structure
fn build_export_tree(tree: &Tree, args: &Args) -> ExportNode {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    let root_path = &args.path;

    // ────────────────────────────────
    //  Build parent → children map
    // ────────────────────────────────
    let mut children_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for entry in &tree.entries {
        let path = entry.path();
        let parent = path.parent().unwrap_or(root_path);

        let rel_parent = parent.strip_prefix(root_path).unwrap_or(parent).to_path_buf();
        let rel_child = path.strip_prefix(root_path).unwrap_or(path).to_path_buf();

        children_map.entry(rel_parent).or_default().push(rel_child);
    }

    // ────────────────────────────────
    //  Recursive function to build nodes
    // ────────────────────────────────
    fn build_node(
        rel_path: &Path,
        root_path: &Path,
        children_map: &HashMap<PathBuf, Vec<PathBuf>>,
        args: &Args,
    ) -> ExportNode {
        let full_path = root_path.join(rel_path);
        let is_dir = full_path.is_dir();
        let metadata = full_path.metadata().ok();

        let size = if args.size || args.info {
            metadata.as_ref().map(|m| m.len())
        } else {
            None
        };

        let permissions = if args.permissions {
            get_permission(metadata)
        } else {
            String::new()
        };

        let display_path = if rel_path.as_os_str().is_empty() {
            format!(
                "./{}",
                root_path.file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            format!(
                "./{}/{}",
                root_path.file_name().unwrap_or_default().to_string_lossy(),
                rel_path.display()
            )
        };

        // Recursively build children
        let mut children_nodes = Vec::new();
        if let Some(children) = children_map.get(rel_path) {
            for child_rel in children {
                let child_node = build_node(child_rel, root_path, children_map, args);
                if args.dirs_only && !child_node.is_dir {
                    continue;
                }
                children_nodes.push(child_node);
            }
        }

        ExportNode {
            name: if rel_path.as_os_str().is_empty() {
                root_path.file_name().unwrap_or_default().to_string_lossy().to_string()
            } else {
                full_path.file_name().unwrap_or_default().to_string_lossy().to_string()
            },
            path: display_path,
            is_dir,
            size,
            dir_count: None,
            file_count: None,
            permissions,
            children: if children_nodes.is_empty() { None } else { Some(children_nodes) },
        }
    }

    // ────────────────────────────────
    // Explicitly build the root node
    // ────────────────────────────────
    build_node(Path::new(""), root_path, &children_map, args)
}
