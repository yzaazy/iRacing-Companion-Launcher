# Definition of done for Rust work: format, lints as errors, tests.
# -Release adds a release build (used by the orchestrator at merges).
param([switch]$Release)

$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Set-Location (Split-Path $PSScriptRoot -Parent)

$steps = [ordered]@{
    'fmt'    = { cargo fmt --all -- --check }
    'clippy' = { cargo clippy --all-targets -- -D warnings }
    'test'   = { cargo test }
}
if ($Release) { $steps['build'] = { cargo build --release } }

foreach ($name in $steps.Keys) {
    Write-Host "== $name"
    & $steps[$name]
    if ($LASTEXITCODE -ne 0) {
        Write-Host "GATE RED: $name"
        exit 1
    }
}
Write-Host 'GATE GREEN'
