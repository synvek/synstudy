pub mod dict_repo;
pub mod progress_repo;

use std::path::PathBuf;

/// Locate the bundled read-only dictionary database.
///
/// Search order (first existing wins):
///   1. `$SYNSTUDY_DICT_DB`
///   2. `<exe_dir>/synstudy.db` and `<exe_dir>/assets/synstudy.db`
///   3. `./assets/synstudy.db` and `./synstudy.db` (relative to CWD; dev mode)
pub fn resolve_dict_db() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(p) = std::env::var("SYNSTUDY_DICT_DB") {
        candidates.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("synstudy.db"));
            candidates.push(dir.join("assets").join("synstudy.db"));
        }
    }
    candidates.push(PathBuf::from("assets/synstudy.db"));
    candidates.push(PathBuf::from("synstudy.db"));

    candidates.into_iter().find(|p| p.exists())
}

/// Path of the writable user-progress database (created on first run).
pub fn progress_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("SYNSTUDY_PROGRESS_DB") {
        let pb = PathBuf::from(p);
        if let Some(dir) = pb.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        return pb;
    }
    let base = directories::ProjectDirs::from("com", "synstudy", "synstudy")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&base);
    base.join("progress.db")
}
