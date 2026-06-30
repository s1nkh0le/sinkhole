//! Small filesystem helpers shared by both binaries.

use std::path::Path;

/// Ensure the parent directory of `path` exists, creating it (recursively) if
/// needed. No-op when `path` has no parent or the parent already exists.
///
/// Used so that a default DB path like `data/sinkhole.db` works out of the box
/// without the operator having to pre-create the directory.
pub fn ensure_parent_dir(path: &str) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
