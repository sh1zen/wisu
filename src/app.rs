use crate::common::sort;
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug, Deserialize)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(override_usage = "wisu [OPTIONS] [PATH]")]
#[derive(Clone)]
pub struct Args {
    /// Start the interactive TUI explorer
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Path to a config file (TOML)
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Export file path
    #[arg(short = 'o', default_value = None, value_parser = clap::builder::PossibleValuesParser::new(["json", "csv", "xml"]))]
    pub out: Option<String>,

    /// The path to the directory to explore/display. Defaults to the current directory.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Show directories only
    #[arg(short = 'd', long)]
    pub dirs_only: bool,

    /// Show directories info
    #[arg(short = 'x', long, default_value = "false")]
    pub info: bool,

    /// Show stats of the scan
    #[arg(long, default_value = "true")]
    pub stats: bool,

    /// Show hyperlinks
    #[arg(short = 'l', long)]
    pub hyperlinks: bool,

    /// Show all files, including hidden ones.
    #[arg(short = 'a', long)]
    pub all: bool,

    /// Respect .gitignore and other standard ignore files.
    #[arg(short = 'g', long)]
    pub gitignore: bool,

    /// Display file-specific icons (requires a Nerd Font)
    #[arg(long)]
    pub icons: bool,

    /// Display the size of files.
    #[arg(short = 's', long)]
    pub size: bool,

    /// Display file permissions.
    #[arg(short = 'p', long)]
    pub permissions: bool,

    /// Initial depth to expand the directory tree (interactive only)
    #[arg(long)]
    pub expand_level: Option<usize>,

    /// Maximum depth to descend in the directory tree (non-interactive only)
    #[arg(short = 'L', long)]
    pub level: Option<usize>,

    /// Maximum files in directory tree (non-interactive only)
    #[arg(short = 'F', long)]
    pub files: Option<usize>,

    /// List only files (non-interactive only)
    #[arg(short = 'f', long)]
    pub files_only: bool,

    /// Sort entries by the specified criteria.
    #[arg(long, default_value_t = SortType::Name)]
    pub sort: SortType,

    /// Sort directories before files.
    #[arg(long)]
    pub dirs_first: bool,

    /// Use case-sensitive sorting.
    #[arg(long)]
    pub case_sensitive: bool,

    /// Use natural/version sorting (e.g., file1 < file10).
    #[arg(long)]
    pub natural_sort: bool,

    /// Reverse the sort order.
    #[arg(short = 'r', long)]
    pub reverse: bool,

    /// Sort dotfiles and dotfolders first.
    #[arg(long)]
    pub dotfiles_first: bool,
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
        if cli.out.is_some() { file.out = cli.out; }
        if cli.expand_level.is_some() { file.expand_level = cli.expand_level; }
        if cli.level.is_some() { file.level = cli.level; }
        if cli.files.is_some() { file.files = cli.files; }
        if cli.config.is_some() { file.config = cli.config; }

        // Path (if different from default)
        if cli.path != PathBuf::from(".") { file.path = cli.path; }

        // Boolean fields: if true in CLI → override
        macro_rules! merge_flag {
            ($field:ident) => {
                if cli.$field {
                    file.$field = true;
                }
            };
        }

        merge_flag!(interactive);
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
