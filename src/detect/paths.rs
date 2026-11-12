//! The common-path search: an app's or game's exe at its usual install locations.

use std::path::Path;

/// The first path that exists on disk, file or directory (`os.path.exists`).
pub fn find_path_in_list(paths: &[String]) -> Option<String> {
    paths.iter().find(|path| Path::new(path).exists()).cloned()
}
