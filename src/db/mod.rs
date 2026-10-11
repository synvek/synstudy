pub mod dict_repo;
pub mod progress_repo;

use std::path::PathBuf;

/// Locate the bundled read-only dictionary database.
///
/// Delegated to [`crate::platform`]: each target resolves it differently —
/// desktop searches sibling + working-dir candidates, iOS reads
/// `Bundle.main.resourceURL`, Android extracts from APK assets into
/// `filesDir` on first launch.
pub fn resolve_dict_db() -> Option<PathBuf> {
    crate::platform::resolve_dict_db()
}

/// Path of the writable user-progress database (created on first run).
pub fn progress_db_path() -> PathBuf {
    crate::platform::progress_db_path()
}
