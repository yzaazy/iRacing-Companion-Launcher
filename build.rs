//! Embeds the exe icon and the Windows version resource. The string fields come from
//! `[package.metadata.winresource]` in `Cargo.toml`; `FileVersion`, `ProductVersion` and the numeric version come from
//! `package.version`, the single version source the installer reads back from the exe.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=iRCL.ico");
    println!("cargo::rerun-if-changed=Cargo.toml");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon("iRCL.ico");
    // English (US): the `040904B0` string table, the one the exe's version resource had up to
    // v2.3.2.
    res.set_language(0x0409);
    if let Err(err) = res.compile() {
        panic!("embedding the Windows resources failed: {err}");
    }
}
