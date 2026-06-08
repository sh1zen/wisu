use ignore::DirEntry;
use std::time::SystemTime;

#[derive(Debug, Clone, Default)]
pub struct CachedMetadata {
    pub size: u64,
    pub accessed: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    #[cfg(unix)]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PreparedEntry {
    pub entry: DirEntry,
    pub metadata: CachedMetadata,
}
