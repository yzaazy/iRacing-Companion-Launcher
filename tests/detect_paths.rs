//! Common-path search.

use iracing_companion_launcher::detect::paths::find_path_in_list;

#[test]
fn returns_the_first_existing_path_file_or_directory() {
    let dir = tempfile::Builder::new()
        .prefix("ircl-paths-ü-")
        .tempdir()
        .unwrap();
    let file = dir.path().join("Trading Paints.exe");
    std::fs::write(&file, b"dummy").unwrap();
    let missing = dir
        .path()
        .join("missing.exe")
        .to_string_lossy()
        .into_owned();
    let file = file.to_string_lossy().into_owned();
    let folder = dir.path().to_string_lossy().into_owned();

    let list = vec![String::new(), missing.clone(), file.clone(), folder.clone()];
    assert_eq!(find_path_in_list(&list), Some(file));
    assert_eq!(
        find_path_in_list(&[missing.clone(), folder.clone()]),
        Some(folder)
    );
    assert_eq!(find_path_in_list(&[missing]), None);
    assert_eq!(find_path_in_list(&[]), None);
}
