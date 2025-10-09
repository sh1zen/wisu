use std::{fs, path::Path};

fn main() {
    let plugins_dir = Path::new("plugins");
    let mod_file_path = plugins_dir.join("plugins_mod.rs");

    let mut contents = String::new();

    // Scansione ricorsiva cartelle (profondità 2: cartella + file plugin)
    for entry in walkdir::WalkDir::new(plugins_dir)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().map(|x| x == "rs").unwrap_or(false)
        })
    {
        let file_name = entry.path().file_stem().unwrap().to_str().unwrap();

        // Salta il file plugins_mod.rs stesso
        if file_name == "plugins_mod" {
            continue;
        }

        // Righe mod + autoregister
        contents.push_str(&format!(
            "mod {file_name}; register_plugin({file_name}::{file_name});\n",
            file_name = file_name
        ));
    }

    fs::write(mod_file_path, contents).unwrap();
    println!("cargo:rerun-if-changed=plugins");
}
