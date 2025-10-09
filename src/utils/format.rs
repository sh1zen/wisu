

/// Formats a size in bytes into a human-readable string using binary prefixes (KiB, MiB).
pub fn size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;

    let bytes = bytes as f64;

    if bytes < KIB {
        format!("{bytes} B")
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes / KIB)
    } else if bytes < GIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes < TIB {
        format!("{:.1} GiB", bytes / GIB)
    } else {
        format!("{:.1} TiB", bytes / TIB)
    }
}

/// Formats a Unix file mode into a human-readable string (e.g., "rwxr-xr-x").
#[cfg(unix)]
pub fn format_permissions(mode: u32) -> String {
    const PERMISSIONS: [(u32, char); 9] = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'), // user
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'), // group
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'), // others
    ];

    PERMISSIONS.iter().map(|&(bit, c)| if mode & bit != 0 { c } else { '-' }).collect()
}

// Unit tests for utility functions
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(size(500), "500 B");
        assert_eq!(size(1024), "1.0 KiB");
        assert_eq!(size(1536), "1.5 KiB");
        let mib = 1024 * 1024;
        assert_eq!(size(mib), "1.0 MiB");
        assert_eq!(size(mib + mib / 2), "1.5 MiB");
        let gib = mib * 1024;
        assert_eq!(size(gib), "1.0 GiB");
    }

    #[test]
    #[cfg(unix)]
    fn test_format_permissions() {
        // -rwxr-xr-x
        let mode = 0o755;
        assert_eq!(format_permissions(mode), "rwxr-xr-x");
        // -rw-r--r--
        let mode_read = 0o644;
        assert_eq!(format_permissions(mode_read), "rw-r--r--");
        // -rwx------
        let mode_user_only = 0o700;
        assert_eq!(format_permissions(mode_user_only), "rwx------");
    }
}
