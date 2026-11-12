# Definition of done for Rust work: format, lints as errors, tests.
# -Release adds a release build and compiles the installer script from it
# (used by the orchestrator at merges). The toolchain is pinned in
# rust-toolchain.toml.
param([switch]$Release)

$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Set-Location (Split-Path $PSScriptRoot -Parent)

$iscc = 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'

# Compiles `windows installer.iss` into a fresh temp directory outside the
# repository, never installer_output\, and deletes it again. The installer is
# only compiled, never run.
function Test-Installer {
    if (-not (Test-Path -LiteralPath $iscc)) {
        Write-Host "SKIPPED: Inno Setup not installed ($iscc); installer script not compiled"
        $global:skipped += 'installer check skipped: Inno Setup not found'
        $global:LASTEXITCODE = 0
        return
    }
    $out = Join-Path ([IO.Path]::GetTempPath()) "ircl-gate-installer-$([guid]::NewGuid())"
    New-Item -ItemType Directory -Path $out | Out-Null
    try {
        & $iscc /Q "/O$out" 'windows installer.iss'
        if ($LASTEXITCODE -eq 0 -and -not (Test-Path -LiteralPath (Join-Path $out 'iRacing_Companion_Launcher_Setup.exe'))) {
            Write-Host "ISCC reported success but wrote no installer to $out"
            $global:LASTEXITCODE = 1
        }
    } finally {
        $code = $LASTEXITCODE
        Remove-Item -LiteralPath $out -Recurse -Force -ErrorAction SilentlyContinue
        $global:LASTEXITCODE = $code
    }
}

$steps = [ordered]@{
    'fmt'    = { cargo fmt --all -- --check }
    'clippy' = { cargo clippy --all-targets -- -D warnings }
    'test'   = { cargo test --no-fail-fast }
}
if ($Release) {
    $steps['build'] = { cargo build --release }
    $steps['installer'] = { Test-Installer }
}

$global:skipped = @()
foreach ($name in $steps.Keys) {
    Write-Host "== $name"
    & $steps[$name]
    if ($LASTEXITCODE -ne 0) {
        Write-Host "GATE RED: $name"
        exit 1
    }
}
# 'GATE GREEN' is the exact last line only when every step ran.
if ($global:skipped) {
    Write-Host "GATE GREEN ($($global:skipped -join '; '))"
} else {
    Write-Host 'GATE GREEN'
}
