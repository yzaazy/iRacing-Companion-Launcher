---
name: release
description: Version bump, release build, installer and GitHub release for the launcher. Use when the user says "bump version", "build the installer", "release", or "publish vX.Y.Z".
---

## Facts

- Semantic versioning, `MAJOR.MINOR.PATCH`. Tags are `v{version}`.
- `version` in `Cargo.toml` is the single source of truth. `build.rs` embeds it, with the icon `iRCL.ico` and the strings from `[package.metadata.winresource]`, into the exe's version resource; the window header reads it through `env!("CARGO_PKG_VERSION")`; `windows installer.iss` reads it back from the built exe's `ProductVersion`. Nothing else holds a version.
- `cargo build --release` writes `target\release\iracing-companion-launcher.exe`, one self-contained file. The installer installs it as `{app}\iRacing Companion Launcher.exe` and is written to `installer_output\iRacing_Companion_Launcher_Setup.exe` (fixed name, not versioned).

## Stop points

- Invoking this skill is the go for steps 1 and 2 (bump, gate). It is not the go for anything after them.
- Building the installer into `installer_output\` (step 4), tagging, pushing (step 6) and publishing the release (step 7) each overwrite something or are seen by others. Each waits for the owner's explicit go in the current conversation; a go for one is not a go for the next.
- Never tag or release from a build whose gate was not green. Never release an installer the owner has not tested (step 5).

## Procedure

Run from the repo root, in this order.

1. **Bump.** If the user did not say, ask: bug (patch `x.y.Z`), feature (minor `x.Y.0`) or major (`X.0.0`, breaking). Check the working tree is clean (`git status --short` prints nothing) and the new tag is free (checks below). Edit `version` in `Cargo.toml`, run `cargo check` so `Cargo.lock` picks up the new version, and commit both files.

2. **Gate.** `pwsh tools/gate.ps1 -Release`. The last line must be exactly `GATE GREEN`: this also builds the release exe and compiles the installer script into a temp directory. `GATE GREEN (installer check skipped: ...)` means Inno Setup is missing; install it before going on. On red, stop and fix.

3. **Embedded version.**
   ```powershell
   (Get-Item target\release\iracing-companion-launcher.exe).VersionInfo.ProductVersion
   ```
   must print the `Cargo.toml` version (for example `3.0.0`).

4. **Installer** (owner's go):
   ```powershell
   & "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" "windows installer.iss"
   ```
   ISCC stops with an error when the release exe is missing or has no version. To compile only for validation, add `/O<temp dir>` so `installer_output\` stays untouched.

5. **The owner tests the installer by hand**: a fresh install, and an upgrade over the previous version (one entry in Installed apps, settings kept, the app starts). Stop here until the owner says it passed.

6. **Tag and push** (owner's go for each): `git tag v{version}` on the commit the installer was built from; push the branch or merge as the owner directs; then `git push origin v{version}`.

7. **Publish** (owner's go):
   ```powershell
   gh release create v{version} "installer_output/iRacing_Companion_Launcher_Setup.exe" --title "v{version}" --generate-notes
   gh release edit v{version} --notes "..."
   ```
   The notes: most important changes only, in user-facing words, in this shape (leave out an empty section):
   ```markdown
   ## What's New

   ### 🚀 New Features
   - ...

   ### 🐛 Bug Fixes
   - ...

   ### ⚡ Improvements
   - ...

   **Full Changelog**: https://github.com/yzaazy/iRacing-Companion-Launcher/compare/v{prev}...v{version}
   ```

The Discord announcement is automatic: GitTrack posts it once the release is published.

## Checks

Run these where the step needs them; any failure stops the procedure.

- Tag free locally and on the remote: `git tag -l v{version}` and `git ls-remote --tags origin v{version}` both print nothing.
- Working tree clean before the bump and before tagging: `git status --short` prints nothing.
- The exe's `ProductVersion` equals `version` in `Cargo.toml` (step 3).
- The installer is newer than the exe it packages:
  ```powershell
  (Get-Item installer_output\iRacing_Companion_Launcher_Setup.exe).LastWriteTime -gt (Get-Item target\release\iracing-companion-launcher.exe).LastWriteTime
  ```
  prints `True`.
- The release does not exist yet: `gh release view v{version}` fails with "release not found".
