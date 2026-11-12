---
name: release
description: Version bump, tag, CI-built installer and GitHub release for the launcher. Use when the user says "bump version", "build the installer", "release", or "publish vX.Y.Z".
---

## Facts

- Semantic versioning, `MAJOR.MINOR.PATCH`. Tags are `v{version}`.
- `version` in `Cargo.toml` is the single source of truth. `build.rs` embeds it, with the icon `iRCL.ico` and the strings from `[package.metadata.winresource]`, into the exe's version resource; the window header reads it through `env!("CARGO_PKG_VERSION")`; `windows installer.iss` reads it back from the built exe's `ProductVersion`. Nothing else holds a version.
- `cargo build --release` writes `target\release\iracing-companion-launcher.exe`, one self-contained file. The installer installs it as `{app}\iRacing Companion Launcher.exe` and is written to `installer_output\iRacing_Companion_Launcher_Setup.exe` (fixed name, not versioned).

## Stop points

- Invoking this skill is the go for steps 1 and 2 (bump, gate). It is not the go for anything after them.
- Pushing the bump branch and opening its pull request (step 3) need no go; the owner merges. Tagging and pushing the tag (step 4) and publishing the release (step 6) are seen by others. Each waits for the owner's explicit go in the current conversation; a go for one is not a go for the next.
- Never tag a commit whose gate was not green. Never publish an installer the owner has not tested (step 5).

## Procedure

Run from the repo root, in this order. The build, the installer and the draft release come from GitHub Actions (`.github/workflows/release.yml`); nothing is uploaded from this machine.

1. **Bump.** If the user did not say, ask: bug (patch `x.y.Z`), feature (minor `x.Y.0`) or major (`X.0.0`, breaking). Check the working tree is clean (`git status --short` prints nothing) and the new tag is free (checks below). Edit `version` in `Cargo.toml`, run `cargo check` so `Cargo.lock` picks up the new version, and commit both files.

2. **Gate.** `pwsh tools/gate.ps1 -Release`. The last line must be exactly `GATE GREEN`: this also builds the release exe and compiles the installer script into a temp directory. `GATE GREEN (installer check skipped: ...)` means Inno Setup is missing locally; CI builds the installer anyway, so this is only a local warning. On red, stop and fix. Then check the embedded version:
   ```powershell
   (Get-Item target\release\iracing-companion-launcher.exe).VersionInfo.ProductVersion
   ```
   must print the `Cargo.toml` version (for example `3.0.0`).

3. **Push**: push the bump on a branch and open a pull request to `main`; `main` takes no direct pushes. The owner merges it (the "Protect Main" ruleset requires the green `Gate` check and the owner's approval as code owner; on their own pull request they merge with the admin bypass). Wait for the CI check on `main` to be green.

4. **Tag and push the tag** (owner's go for each): `git fetch`, then `git tag v{version} origin/main` once the bump is merged (releases are made only from commits on `main`), then `git push origin v{version}`. The Release workflow then fails if the tagged commit is not on `main`, if the tag differs from `version` in `Cargo.toml` or a release for it exists, and otherwise runs `cargo audit`, the gate, `cargo build --release`, checks the exe's version, compiles the installer and creates a **draft** release for the tag with the installer attached and generated notes. Watch it with `gh run watch` or on the Actions tab. When it fails after creating nothing, fix the cause, move the tag (`git tag -f`, `git push -f origin v{version}`, owner's go) or delete it, and push again; when it left a draft, delete the draft first.

5. **The owner tests the installer by hand**: download it from the draft (`gh release download v{version} --dir <temp dir>`), then a fresh install, and an upgrade over the previous version (one entry in Installed apps, settings kept, the app starts). Stop here until the owner says it passed.

6. **Publish** (owner's go): write the notes, then publish the draft.
   ```powershell
   gh release edit v{version} --notes "..."
   gh release edit v{version} --draft=false
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

**Without CI** (Actions down): after step 2, compile the installer with `& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" "windows installer.iss"` (writes `installer_output\`, owner's go), have it tested, tag and push the tag only after disabling the Release workflow (`gh workflow disable release.yml`), then `gh release create v{version} "installer_output/iRacing_Companion_Launcher_Setup.exe" --title "v{version}" --notes "..."` (owner's go) and re-enable the workflow.

## Checks

Run these where the step needs them; any failure stops the procedure.

- Tag free locally and on the remote: `git tag -l v{version}` and `git ls-remote --tags origin v{version}` both print nothing.
- Working tree clean before the bump and before tagging: `git status --short` prints nothing.
- The exe's `ProductVersion` equals `version` in `Cargo.toml` (step 2; the Release workflow checks it again on its own build).
- No release or draft exists for the tag before step 4: `gh release view v{version}` fails with "release not found".
- Before step 5: `gh release view v{version}` shows the release as a draft with `iRacing_Companion_Launcher_Setup.exe` attached.
