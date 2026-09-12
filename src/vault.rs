//! Vault location, config persistence, and the path-resolution security boundary.

use crate::error::{AppError, AppResult};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const CONFIG_FILENAME: &str = "possess-config.json";
pub const DEFAULT_VAULT_DIRNAME: &str = "notes";

/// Where index.html, css/, js/ and the config file live.
///
/// The Python version used the script's own directory. A compiled binary sits
/// in target/debug during development, so fall back to the working directory
/// when the executable has no index.html beside it.
pub fn root_dir() -> PathBuf {
    if let Some(explicit) = std::env::var_os("POSSESS_ROOT") {
        return PathBuf::from(explicit);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("index.html").is_file() {
                return dir.to_path_buf();
            }
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn config_file() -> PathBuf {
    root_dir().join(CONFIG_FILENAME)
}

pub fn default_vault() -> PathBuf {
    root_dir().join(DEFAULT_VAULT_DIRNAME)
}

/// The whole config file, or an empty one if it's missing or corrupt.
pub fn load_config() -> Map<String, Value> {
    match std::fs::read_to_string(config_file()) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        },
        Err(_) => Map::new(),
    }
}

/// Merge `updates` into the config file, leaving other keys alone.
pub fn save_config(updates: Vec<(&str, Value)>) {
    let mut config = load_config();
    for (key, value) in updates {
        config.insert(key.to_string(), value);
    }

    let text = serde_json::to_string_pretty(&Value::Object(config)).unwrap_or_default();
    if let Err(err) = std::fs::write(config_file(), text) {
        eprintln!("[PossessApp] Could not write config: {err}");
    }
}

/// Read the remembered vault, falling back to notes/ if it's gone.
pub fn load_saved_vault() -> PathBuf {
    if let Some(Value::String(saved)) = load_config().get("vault") {
        let path = PathBuf::from(saved);
        if path.is_dir() {
            return canonical(&path);
        }
    }
    default_vault()
}

pub fn save_vault(path: &Path) {
    save_config(vec![("vault", Value::String(path.display().to_string()))]);
}

/// Whether save-hooks may run for this vault.
///
/// Keyed by vault path and defaulting to false: opening a vault someone else
/// prepared must never start executing their Python on your machine because a
/// different vault had hooks turned on.
pub fn hooks_enabled(vault: &Path) -> bool {
    let wanted = vault.display().to_string();
    matches!(load_config().get("hooks_enabled_for"), Some(Value::Array(list))
        if list.iter().any(|v| v.as_str() == Some(wanted.as_str())))
}

pub fn set_hooks_enabled(vault: &Path, enabled: bool) {
    let wanted = vault.display().to_string();

    let mut allowed: Vec<String> = match load_config().get("hooks_enabled_for") {
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };

    allowed.retain(|p| p != &wanted);
    if enabled {
        allowed.push(wanted);
    }
    allowed.sort();

    save_config(vec![(
        "hooks_enabled_for",
        Value::Array(allowed.into_iter().map(Value::String).collect()),
    )]);
}

/// Canonicalize, stripping Windows' \\?\ prefix. Falls back to the path as
/// given when it doesn't exist yet.
pub fn canonical(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(real) => dunce::simplified(&real).to_path_buf(),
        Err(_) => path.to_path_buf(),
    }
}

/// Resolve a client-supplied relative path inside the vault.
///
/// Everything arriving from the browser is untrusted, so the resolved path must
/// be proven to sit under the vault before any read or write.
///
/// Python's Path.resolve() collapses '..' AND follows symlinks, and it does so
/// happily on paths that don't exist yet. Rust's fs::canonicalize refuses a
/// path that doesn't exist (ENOENT) — but /api/create and POST /api/file both
/// resolve paths before creating them, so canonicalize alone cannot be used.
///
/// So walk the path one component at a time: pop on '..', and canonicalize each
/// component that does exist. That resolves a symlink the moment we step onto
/// it, which is what stops a link inside the vault pointing outside it, while
/// still producing a path for components that have yet to be created.
pub fn resolve_in_vault(vault: &Path, filepath: &str) -> AppResult<PathBuf> {
    let vault = canonical(vault);
    let mut current = vault.clone();

    for part in filepath.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => {
                current.pop();
            }
            _ => {
                current.push(part);
                // Only canonicalize what already exists; a component that does
                // not is appended lexically and checked by starts_with below.
                if current.exists() {
                    current = canonical(&current);
                }
            }
        }
    }

    if !current.starts_with(&vault) {
        return Err(AppError::forbidden("Access denied"));
    }
    Ok(current)
}

/// Drive letters, so the folder picker isn't stranded on one volume.
pub fn windows_drives() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }

    ('A'..='Z')
        .map(|d| format!("{d}:\\"))
        .filter(|p| Path::new(p).exists())
        .collect()
}

/// The folder every client-supplied path is resolved inside. Swapped at runtime
/// via POST /api/vault and remembered across restarts in the config file.
#[derive(Clone)]
pub struct AppState {
    vault: Arc<RwLock<PathBuf>>,
    pub hook_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub fn new(vault: PathBuf) -> Self {
        Self {
            vault: Arc::new(RwLock::new(vault)),
            hook_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn vault(&self) -> PathBuf {
        self.vault.read().expect("vault lock poisoned").clone()
    }

    /// The vault as an absolute, symlink-resolved path — what every containment
    /// check compares against.
    pub fn vault_resolved(&self) -> PathBuf {
        canonical(&self.vault())
    }

    pub fn set_vault(&self, path: PathBuf) {
        *self.vault.write().expect("vault lock poisoned") = path;
    }

    pub fn resolve(&self, filepath: &str) -> AppResult<PathBuf> {
        resolve_in_vault(&self.vault(), filepath)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vault with a file, a nested folder, and a symlink pointing outside.
    fn fixture() -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("possess-test-{}", uuid::Uuid::new_v4()));
        let vault = base.join("vault");
        let outside = base.join("outside");

        std::fs::create_dir_all(vault.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(vault.join("note.md"), "hi").unwrap();
        std::fs::write(outside.join("secret.md"), "secret").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, vault.join("escape")).unwrap();

        (base, vault)
    }

    #[test]
    fn resolves_a_plain_path() {
        let (_base, vault) = fixture();
        let got = resolve_in_vault(&vault, "note.md").unwrap();
        assert_eq!(got, canonical(&vault).join("note.md"));
    }

    #[test]
    fn resolves_a_path_that_does_not_exist_yet() {
        // The case fs::canonicalize cannot handle: /api/create resolves before
        // creating. Python's Path.resolve() allows it, so we must too.
        let (_base, vault) = fixture();
        let got = resolve_in_vault(&vault, "sub/brand-new.md").unwrap();
        assert_eq!(got, canonical(&vault).join("sub").join("brand-new.md"));
    }

    #[test]
    fn rejects_dot_dot_traversal() {
        let (_base, vault) = fixture();
        assert!(resolve_in_vault(&vault, "../outside/secret.md").is_err());
        assert!(resolve_in_vault(&vault, "../../etc/passwd").is_err());
        assert!(resolve_in_vault(&vault, "sub/../../outside/secret.md").is_err());
    }

    #[test]
    fn rejects_a_traversal_into_a_path_that_does_not_exist() {
        let (_base, vault) = fixture();
        assert!(resolve_in_vault(&vault, "../escape-to-here.md").is_err());
    }

    #[test]
    #[cfg(unix)]
    fn rejects_a_symlink_pointing_out_of_the_vault() {
        // The reason we canonicalize each existing component instead of only
        // normalising lexically: "escape" is inside the vault by name.
        let (_base, vault) = fixture();
        assert!(resolve_in_vault(&vault, "escape/secret.md").is_err());
        assert!(resolve_in_vault(&vault, "escape").is_err());
    }

    #[test]
    fn tolerates_redundant_separators_and_dots() {
        let (_base, vault) = fixture();
        let want = canonical(&vault).join("note.md");
        assert_eq!(resolve_in_vault(&vault, "./note.md").unwrap(), want);
        assert_eq!(resolve_in_vault(&vault, "sub/.././note.md").unwrap(), want);
        assert_eq!(resolve_in_vault(&vault, "//note.md").unwrap(), want);
    }

    #[test]
    fn an_empty_path_is_the_vault_itself() {
        let (_base, vault) = fixture();
        assert_eq!(resolve_in_vault(&vault, "").unwrap(), canonical(&vault));
    }
}
