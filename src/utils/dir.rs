use std::fs::Metadata;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Restituisce un percorso canonico/assoluto cross-platform senza prefisso \\?\ su Windows
/// Il percorso passato deve essere già assoluto
#[inline]
pub fn canonicalize_path(path: &Path) -> PathBuf {
    // Tentativo canonicalize, fallback normalizzazione
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| normalize_path(path));

    // Rimuove prefisso \\?\ su Windows
    #[cfg(windows)]
    {
        let s = abs.as_os_str().to_string_lossy();
        PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
    }
    #[cfg(not(windows))]
    {
        abs
    }
}

/// Normalizza il percorso rimuovendo "." e ".." senza controllare l'esistenza
#[inline]
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut comps = Vec::with_capacity(path.components().count());
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                comps.pop();
            }
            other => comps.push(other),
        }
    }
    comps.iter().collect()
}

#[inline]
pub fn get_permission(metadata: Option<Metadata>) -> String {
    let perms = if let Some(md) = metadata {
        #[cfg(unix)]
        {
            let mode = md.permissions().mode();
            let ft_char = if md.is_dir() { 'd' } else { '-' };
            format!("{}{}", ft_char, super::format::format_permissions(mode))
        }
        #[cfg(not(unix))]
        {
            let _ = md;
            "----------".to_string()
        }
    } else {
        "----------".to_string()
    };
    format!("{perms} ")
}
