//! Start Menu shortcut search against fake trees with real `.lnk` files.

mod detect_support;

use detect_support::{Tree, s};
use iracing_companion_launcher::detect::shortcuts::{
    find_shortcut_target, find_shortcut_target_in,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoInitializeEx};

const NAMES: &[&str] = &["Fanatec.lnk", "Fanatec Control Panel.lnk"];

#[test]
fn app_shortcut_nested_several_folders_deep_resolves_to_target() {
    let tree = Tree::new();
    let exe = tree.target(r"Fanatec\Fanatec.exe");
    tree.lnk(&tree.root2(), r"Vendor\Tools\Wheel\Deep\Fanatec.lnk", &exe);

    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&exe))
    );
}

#[test]
fn file_name_matches_in_any_case_and_keeps_pattern_casing() {
    let tree = Tree::new();
    let exe = tree.target("Fanatec.exe");
    tree.lnk(&tree.root1(), r"Sub\FANATEC.LNK", &exe);

    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&exe))
    );
    // rglob yields the literal pattern joined onto the directory, not the
    // name as it is stored on disk.
    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, true),
        Some(s(&tree.root1().join(r"Sub\Fanatec.lnk")))
    );
}

#[test]
fn first_root_wins_over_an_earlier_name_in_the_second_root() {
    let tree = Tree::new();
    let first = tree.target("first.exe");
    let second = tree.target("second.exe");
    tree.lnk(&tree.root1(), r"A\Fanatec Control Panel.lnk", &first);
    tree.lnk(&tree.root2(), "Fanatec.lnk", &second);

    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&first))
    );
}

#[test]
fn first_name_wins_over_a_shallower_later_name_in_the_same_root() {
    let tree = Tree::new();
    let first = tree.target("first.exe");
    let second = tree.target("second.exe");
    tree.lnk(&tree.root1(), r"A\B\C\Fanatec.lnk", &first);
    tree.lnk(&tree.root1(), "Fanatec Control Panel.lnk", &second);

    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&first))
    );
}

#[test]
fn walk_order_matches_python_rglob() {
    // The root's own entry comes first.
    let tree = Tree::new();
    let top = tree.target("top.exe");
    let nested = tree.target("nested.exe");
    tree.lnk(&tree.root1(), r"A\Fanatec.lnk", &nested);
    tree.lnk(&tree.root1(), "Fanatec.lnk", &top);
    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&top))
    );

    // Children are checked in listing order before any of them is entered:
    // B\Fanatec.lnk beats A\X\Fanatec.lnk.
    let tree = Tree::new();
    let a = tree.target("a.exe");
    let b = tree.target("b.exe");
    tree.lnk(&tree.root1(), r"A\X\Fanatec.lnk", &a);
    tree.lnk(&tree.root1(), r"B\Fanatec.lnk", &b);
    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&b))
    );

    // Descent is last-in first-out: B's subtree is walked before A's.
    let tree = Tree::new();
    let a = tree.target("a.exe");
    let b = tree.target("b.exe");
    tree.lnk(&tree.root1(), r"A\X\Fanatec.lnk", &a);
    tree.lnk(&tree.root1(), r"B\Y\Fanatec.lnk", &b);
    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&b))
    );

    // Siblings at the same depth go in listing order.
    let tree = Tree::new();
    let a = tree.target("a.exe");
    let b = tree.target("b.exe");
    tree.lnk(&tree.root1(), r"A\Fanatec.lnk", &a);
    tree.lnk(&tree.root1(), r"B\Fanatec.lnk", &b);
    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&a))
    );
}

#[test]
fn shortcut_with_missing_target_is_skipped_for_a_later_valid_one() {
    let tree = Tree::new();
    let gone = tree.target("gone.exe");
    let valid = tree.target("valid.exe");
    tree.lnk(&tree.root1(), r"A\Fanatec.lnk", &gone);
    tree.lnk(&tree.root1(), r"B\Fanatec.lnk", &valid);
    std::fs::remove_file(&gone).unwrap();

    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&valid))
    );
}

#[test]
fn game_mode_returns_the_lnk_path_without_resolving_it() {
    let tree = Tree::new();
    let gone = tree.target("iRacingUI.exe");
    let lnk = tree.lnk(&tree.root2(), r"iRacing\iRacing.lnk", &gone);
    std::fs::remove_file(&gone).unwrap();

    let names = &["iRacing.lnk", "iRacing Simulator.lnk"];
    assert_eq!(
        find_shortcut_target_in(&tree.roots(), names, true),
        Some(s(&lnk))
    );
    assert_eq!(find_shortcut_target_in(&tree.roots(), names, false), None);
}

#[test]
fn non_ascii_paths_round_trip() {
    let tree = Tree::with_prefix("ircl-détect-日本-");
    let exe = tree.target(r"Prögrâm Fïles\Ünïcødé\Fanatec.exe");
    let lnk = tree.lnk(&tree.root1(), r"Startmenü\Ordner ß\Fanatec.lnk", &exe);

    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&exe))
    );
    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, true),
        Some(s(&lnk))
    );
}

#[test]
fn corrupt_lnk_and_directory_named_like_a_shortcut_are_skipped() {
    let tree = Tree::new();
    let valid = tree.target("valid.exe");
    std::fs::create_dir_all(tree.root1().join("A")).unwrap();
    std::fs::write(tree.root1().join(r"A\Fanatec.lnk"), b"not a shell link").unwrap();
    std::fs::create_dir_all(tree.root1().join(r"B\Fanatec.lnk")).unwrap();
    tree.lnk(&tree.root1(), r"C\Fanatec.lnk", &valid);

    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&valid))
    );
    // Games only need the match to exist, so the corrupt file is returned.
    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, true),
        Some(s(&tree.root1().join(r"A\Fanatec.lnk")))
    );
}

#[test]
fn empty_lnk_file_is_skipped() {
    let tree = Tree::new();
    std::fs::write(tree.root1().join("Fanatec.lnk"), b"").unwrap();

    assert_eq!(find_shortcut_target_in(&tree.roots(), NAMES, false), None);
}

#[test]
fn nothing_found() {
    let tree = Tree::new();
    let exe = tree.target("other.exe");
    tree.lnk(&tree.root1(), r"A\Other.lnk", &exe);
    let missing_root = tree.dir.path().join("does-not-exist");

    let roots = vec![missing_root, tree.root1(), tree.root2()];
    assert_eq!(find_shortcut_target_in(&roots, NAMES, false), None);
    assert_eq!(find_shortcut_target_in(&roots, NAMES, true), None);
    assert_eq!(find_shortcut_target_in(&roots, &[], false), None);
}

#[test]
fn junctions_are_walked_like_directories() {
    let tree = Tree::new();
    let exe = tree.target("Fanatec.exe");
    let outside = tree.dir.path().join("outside");
    tree.lnk(&outside, "Fanatec.lnk", &exe);
    let junction = tree.root1().join("Linked");
    let made = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");

    assert_eq!(
        find_shortcut_target_in(&tree.roots(), NAMES, false),
        Some(s(&exe))
    );
}

#[test]
fn works_on_threads_with_and_without_com() {
    let tree = Tree::new();
    let exe = tree.target("Fanatec.exe");
    tree.lnk(&tree.root1(), r"A\Fanatec.lnk", &exe);
    let expected = Some(s(&exe));
    let roots = tree.roots();
    let roots = &roots;

    std::thread::scope(|scope| {
        let none = scope.spawn(|| {
            (0..3)
                .map(|_| find_shortcut_target_in(roots, NAMES, false))
                .collect::<Vec<_>>()
        });
        let sta = scope.spawn(|| {
            let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            find_shortcut_target_in(roots, NAMES, false)
        });
        let mta = scope.spawn(|| {
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let found = find_shortcut_target_in(roots, NAMES, false);
            // The thread must still be multithreaded afterwards: another MTA
            // init succeeds, where a leftover STA would refuse it.
            assert!(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok());
            found
        });
        assert_eq!(none.join().unwrap(), vec![expected.clone(); 3]);
        assert_eq!(sta.join().unwrap(), expected);
        assert_eq!(mta.join().unwrap(), expected);
    });
}

#[test]
fn real_start_menu_search_does_not_fail() {
    // Read-only: the real roots are walked and nothing matches this name.
    assert_eq!(
        find_shortcut_target(&["ircl-no-such-shortcut-7f3a.lnk"], false),
        None
    );
    assert_eq!(
        find_shortcut_target(&["ircl-no-such-shortcut-7f3a.lnk"], true),
        None
    );
}
