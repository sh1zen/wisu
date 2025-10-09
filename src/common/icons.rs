use std::path::Path;

/// Returns Emoji icon for a given file path.
///
/// The selection logic  checks for file extensions.
///
/// # Arguments
///
/// * `path` - A reference to the `Path` of the file or directory.
/// * `is_dir` - A boolean indicating if the `path` is a directory.
///
/// # Returns
/// * `String` - The Emoji icon.
pub fn get_icon_for_path(path: &Path, is_dir: bool) -> String {
    if is_dir {
        return "📁".to_string(); // Cartella
    }

    // Estensione del file
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

    // Icone basate sul tipo di file
    let icon = match ext.as_str() {
        // --- Linguaggi di programmazione ---
        "rs" => "🦀",                                     // Rust
        "py" => "🐍",                                     // Python
        "js" | "mjs" => "🧩",                             // JavaScript
        "ts" | "tsx" => "🧠",                             // TypeScript
        "java" => "☕",                                   // Java
        "cpp" | "cc" | "cxx" | "hpp" | "h" | "c" => "⚙️", // C/C++
        "go" => "🐹",                                     // Go
        "php" => "🐘",                                    // PHP
        "rb" => "💎",                                     // Ruby
        "swift" => "🕊️",                                  // Swift
        "kt" | "kts" => "🤖",                             // Kotlin
        "dart" => "🎯",                                   // Dart
        "lua" => "🌙",                                    // Lua
        "html" => "🌐",
        "css" | "scss" | "less" => "🎨",
        "sql" => "🗄️", // SQL

        // --- Configurazioni e script ---
        "toml" | "yaml" | "yml" | "json" | "ini" => "⚙️",
        "lock" => "🔒",
        "sh" | "bash" | "zsh" | "ps1" => "💻",
        "env" => "🌱",
        "dockerfile" => "🐳",
        "makefile" | "mk" => "🔧",

        // --- Documenti ---
        "md" | "markdown" => "📝",
        "txt" => "📄",
        "pdf" => "📕",
        "doc" | "docx" => "📘",
        "xls" | "xlsx" | "ods" => "📗",
        "ppt" | "pptx" | "odp" => "📙",
        "rtf" => "📜",

        // --- Archivi ---
        "zip" | "gz" | "tar" | "rar" | "7z" | "bz2" => "🗜️",
        "iso" => "💿",

        // --- Immagini e grafica ---
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "ico" | "webp" => "🖼️",
        "psd" | "xcf" => "🎨",

        // --- Audio e Video ---
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => "🎵",
        "mp4" | "mkv" | "avi" | "mov" | "webm" => "🎞️",
        "srt" | "vtt" => "💬",

        // --- Code e dati ---
        "csv" | "tsv" | "xml" => "📊",
        "db" | "sqlite" | "db3" => "🗃️",
        "log" => "📜",

        // --- Eseguibili e sistema ---
        "exe" | "bin" | "app" | "msi" => "⚡",
        "dll" | "so" | "dylib" => "🧱",
        "bat" | "cmd" => "🪟",

        // --- Web e network ---
        "jsonl" | "ndjson" => "🌐",
        "wasm" => "🧬",
        "pem" | "crt" | "cer" | "key" => "🔐",

        "conf" | "cfg" => "🧩",

        // Default
        _ => "📄",
    };

    icon.to_string()
}
