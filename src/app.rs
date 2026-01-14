use crate::common::sort;
use chrono::{Duration, NaiveDate, Utc};
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Parser, Debug, Deserialize)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(override_usage = "wisu [OPTIONS] [PATH]")]
#[derive(Clone)]
pub struct Args {
    /* =========================
     * Execution mode
     * ========================= */
    /// Start the interactive TUI explorer
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Watch for filesystem changes and auto-refresh
    #[arg(long, default_value = "false")]
    pub watch: bool,

    /* =========================
     * Input / configuration
     * ========================= */
    /// Path to a config file (TOML)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Path to the directory to explore/display
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /* =========================
     * Output / export
     * ========================= */
    /// Export output format (json, csv, xml)
    #[arg(short = 'o', default_value = None, value_parser = clap::builder::PossibleValuesParser::new(["json", "csv", "xml"]))]
    pub out: Option<String>,

    /* =========================
     * Content filters
     * ========================= */
    /// Show directories only
    #[arg(short = 'd', long)]
    pub dirs_only: bool,

    /// List only files (non-interactive only)
    #[arg(short = 'f', long)]
    pub files_only: bool,

    /// Show all files, including hidden ones
    #[arg(short = 'a', long)]
    pub all: bool,

    /// Respect .gitignore and other ignore files
    #[arg(short = 'g', long)]
    pub gitignore: bool,

    /// Exclude files by extension (comma-separated, e.g. "log,tmp")
    #[arg(short = 'e', long)]
    pub exclude: Option<String>,

    /// Time filter (relative or absolute date)
    ///
    /// Relative: 5d, 2w, 3M, 1y, 30s, 10m
    /// Absolute:
    ///   YYYY-MM-DD   → after date
    ///  -YYYY-MM-DD   → before date
    #[arg(short = 't', long)]
    pub time: Option<TimeFilter>,

    /* =========================
     * Depth & limits
     * ========================= */
    /// Initial depth to expand the tree (interactive only)
    #[arg(long)]
    pub expand_level: Option<usize>,

    /// Maximum directory depth (non-interactive only)
    #[arg(short = 'L', long)]
    pub level: Option<usize>,

    /// Maximum number of files (non-interactive only)
    #[arg(short = 'F', long)]
    pub files: Option<usize>,

    /* =========================
     * Sorting
     * ========================= */
    /// Sort entries by criteria
    #[arg(long, default_value_t = SortType::Name)]
    pub sort: SortType,

    /// Reverse sort order
    #[arg(short = 'r', long)]
    pub reverse: bool,

    /// Sort directories before files
    #[arg(long)]
    pub dirs_first: bool,

    /// Use case-sensitive sorting
    #[arg(long)]
    pub case_sensitive: bool,

    /// Use natural/version sorting (file2 < file10)
    #[arg(long)]
    pub natural_sort: bool,

    /// Sort dotfiles and dotfolders first
    #[arg(long)]
    pub dotfiles_first: bool,

    /* =========================
     * Display options
     * ========================= */
    /// Show hyperlinks
    #[arg(short = 'l', long)]
    pub hyperlinks: bool,

    /// Display file-specific icons (requires Nerd Font)
    #[arg(long)]
    pub icons: bool,

    /* =========================
     * Metadata & details
     * ========================= */
    /// Show file sizes
    #[arg(short = 's', long)]
    pub size: bool,

    /// Show file permissions
    #[arg(short = 'p', long)]
    pub permissions: bool,

    /// Show extended directory info
    #[arg(short = 'x', long, default_value = "false")]
    pub info: bool,

    /// Show scan statistics
    #[arg(long, default_value = "true")]
    pub stats: bool,
}

impl Args {
    /// Load `Args` from CLI + TOML file (if it exists).
    /// CLI values override those from the file.
    pub fn load() -> Self {
        let cli_args = Args::parse(); // read CLI

        if let Some(config_path) = cli_args.config.clone() {
            if let Some(mut file_args) = Self::from_file(&config_path) {
                file_args = Self::merge(file_args, cli_args);
                return file_args;
            }
        }

        // Otherwise, look for `wisu.toml` in the provided path
        let candidate = cli_args.path.join("wisu.toml");
        if let Some(mut file_args) = Self::from_file(&candidate) {
            file_args = Self::merge(file_args, cli_args);
            return file_args;
        }

        cli_args
    }

    fn from_file(path: &Path) -> Option<Self> {
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(path).ok()?;
        toml::from_str::<Args>(&content).ok()
    }

    /// Merge two Args: CLI values override those from the file
    fn merge(mut file: Args, cli: Args) -> Args {
        // Optional options
        if cli.out.is_some() {
            file.out = cli.out;
        }
        if cli.expand_level.is_some() {
            file.expand_level = cli.expand_level;
        }
        if cli.level.is_some() {
            file.level = cli.level;
        }
        if cli.files.is_some() {
            file.files = cli.files;
        }
        if cli.config.is_some() {
            file.config = cli.config;
        }
        if cli.time.is_some() {
            file.time = cli.time;
        }
        if cli.exclude.is_some() {
            file.exclude = cli.exclude;
        }

        // Path (if different from default)
        if cli.path != PathBuf::from(".") {
            file.path = cli.path;
        }

        // Boolean fields: if true in CLI → override
        macro_rules! merge_flag {
            ($field:ident) => {
                if cli.$field {
                    file.$field = true;
                }
            };
        }

        merge_flag!(interactive);
        merge_flag!(watch);
        merge_flag!(dirs_only);
        merge_flag!(info);
        merge_flag!(stats);
        merge_flag!(hyperlinks);
        merge_flag!(all);
        merge_flag!(gitignore);
        merge_flag!(icons);
        merge_flag!(size);
        merge_flag!(permissions);
        merge_flag!(files_only);
        merge_flag!(dirs_first);
        merge_flag!(case_sensitive);
        merge_flag!(natural_sort);
        merge_flag!(reverse);
        merge_flag!(dotfiles_first);

        // Enum or other fields with defaults
        file.sort = cli.sort;

        file
    }

    /// Get the set of excluded extensions (normalized to lowercase)
    pub fn get_excluded_extensions(&self) -> std::collections::HashSet<String> {
        let mut excluded = std::collections::HashSet::new();

        if let Some(ref exclude_str) = self.exclude {
            for ext in exclude_str.split(',') {
                let normalized = ext.trim().to_lowercase();
                if !normalized.is_empty() {
                    // Remove leading dot if present
                    let clean_ext = if normalized.starts_with('.') {
                        normalized[1..].to_string()
                    } else {
                        normalized
                    };
                    excluded.insert(clean_ext);
                }
            }
        }

        excluded
    }

    /// Check if a file should be excluded based on its extension
    pub fn is_excluded(&self, path: &Path) -> bool {
        if self.exclude.is_none() {
            return false;
        }

        let excluded = self.get_excluded_extensions();

        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                return excluded.contains(&ext_str.to_lowercase());
            }
        }

        false
    }
}

/// Represents a time-based filter for files
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct TimeFilter {
    pub mode: TimeFilterMode,
    pub threshold: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TimeFilterMode {
    After,  // file modificato dopo questa data
    Before, // file modificato prima di questa data
}

impl TimeFilter {
    /// Check if a file timestamp matches this filter
    pub fn matches(&self, file_time: chrono::DateTime<Utc>) -> bool {
        match self.mode {
            TimeFilterMode::After => file_time >= self.threshold,
            TimeFilterMode::Before => file_time < self.threshold,
        }
    }
}

impl FromStr for TimeFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("Empty time filter".to_string());
        }

        let now = Utc::now();

        // Check for before (<) or after (>) prefix
        let (mode, date_part) = if s.starts_with('<') {
            (TimeFilterMode::Before, &s[1..])
        } else if s.starts_with('>') {
            (TimeFilterMode::After, &s[1..])
        } else {
            (TimeFilterMode::After, s) // default: after
        };

        // Try parsing as absolute date (multiple formats)
        let parse_date = |date_str: &str| -> Option<NaiveDate> {
            // Try dd-mm-yyyy, dd/mm/yyyy, yyyy-mm-dd
            NaiveDate::parse_from_str(date_str, "%d-%m-%Y")
                .or_else(|_| NaiveDate::parse_from_str(date_str, "%d/%m/%Y"))
                .or_else(|_| NaiveDate::parse_from_str(date_str, "%Y-%m-%d"))
                .ok()
        };

        // Try parsing as date
        if let Some(date) = parse_date(date_part) {
            let dt = date.and_hms_opt(0, 0, 0).unwrap();
            return Ok(TimeFilter {
                mode,
                threshold: chrono::DateTime::from_naive_utc_and_offset(dt, Utc),
            });
        }

        // If had a prefix but couldn't parse date, error
        if s.starts_with('<') || s.starts_with('>') {
            return Err(format!(
                "Invalid date format: {}. Use dd-mm-yyyy, dd/mm/yyyy or yyyy-mm-dd",
                date_part
            ));
        }

        // Parse relative time: number + unit
        let last_char = s.chars().last().ok_or("Empty time filter")?;
        if !last_char.is_ascii_alphabetic() {
            return Err(format!(
                "Invalid time filter: {}. Use relative (5d, 2w, 3M) or date (dd-mm-yyyy)",
                s
            ));
        }

        let (num_str, unit) = s.split_at(s.len() - 1);
        let num: i64 = num_str.parse().map_err(|_| {
            format!("Invalid time filter: {}. Use relative (5d, 2w, 3M) or date (dd-mm-yyyy)", s)
        })?;

        let duration = match unit {
            "s" => Duration::seconds(num),
            "m" => Duration::minutes(num),
            "h" => Duration::hours(num),
            "d" => Duration::days(num),
            "w" => Duration::weeks(num),
            "M" => Duration::days(num * 30),
            "y" => Duration::days(num * 365),
            _ => return Err(format!("Unknown time unit: {}. Use s/m/h/d/w/M/y", unit)),
        };

        Ok(TimeFilter { mode: TimeFilterMode::After, threshold: now - duration })
    }
}

impl TryFrom<String> for TimeFilter {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl fmt::Display for TimeFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self.mode {
            TimeFilterMode::Before => "before ",
            TimeFilterMode::After => "after ",
        };
        write!(f, "{}{}", prefix, self.threshold.format("%Y-%m-%d %H:%M:%S"))
    }
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq, Default, Deserialize)]
pub enum SortType {
    #[default]
    Name,
    Size,
    Accessed,
    Created,
    Modified,
    Extension,
}

impl From<SortType> for sort::SortType {
    fn from(sort_type: SortType) -> Self {
        match sort_type {
            SortType::Name => sort::SortType::Name,
            SortType::Size => sort::SortType::Size,
            SortType::Accessed => sort::SortType::Accessed,
            SortType::Created => sort::SortType::Created,
            SortType::Modified => sort::SortType::Modified,
            SortType::Extension => sort::SortType::Extension,
        }
    }
}

impl Args {
    pub fn to_sort_options(&self) -> sort::SortOptions {
        sort::SortOptions {
            sort_type: self.sort.into(),
            directories_first: self.dirs_first,
            case_sensitive: self.case_sensitive,
            natural_sort: self.natural_sort,
            reverse: self.reverse,
            dotfiles_first: self.dotfiles_first,
        }
    }
}

impl fmt::Display for SortType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_possible_value().expect("no values are skipped").get_name().fmt(f)
    }
}
