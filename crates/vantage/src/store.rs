//! Path display helpers shared with the engine adapter.

use std::path::Path;

/// Collapse the home directory back to `~` for display.
pub fn contract(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.to_string_lossy());
        }
    }
    path.to_string_lossy().to_string()
}
