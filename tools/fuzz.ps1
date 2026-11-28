<#
.SYNOPSIS
  Coverage-guided fuzzing of the file readers (nightly + cargo-fuzz, not part of the normal CI).
  Corpus and crashes stay in fuzz\corpus / fuzz\artifacts (git-ignored).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\fuzz.ps1 -Target hdri -Seconds 120
#>
param(
    [ValidateSet('hdri', 'bimc', 'las', 'all')]
    [string]$Target = 'all',
    [int]$Seconds = 300
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location (Join-Path $root 'fuzz')

if (-not (Get-Command cargo-fuzz -ErrorAction SilentlyContinue)) {
    throw 'cargo-fuzz is missing. Install it with: cargo install cargo-fuzz'
}
$toolchain = (& rustc +nightly --version 2>$null)
if (-not $toolchain) {
    throw 'The nightly toolchain is missing. Install it with: rustup toolchain install nightly'
}
"Using $toolchain"

$targets = if ($Target -eq 'all') { @('hdri', 'bimc', 'las') } else { @($Target) }
foreach ($t in $targets) {
    Write-Host "`n==> fuzzing $t for $Seconds s" -ForegroundColor Cyan
    & cargo +nightly fuzz run $t -- -max_total_time=$Seconds -rss_limit_mb=4096
    if ($LASTEXITCODE -ne 0) { throw "cargo-fuzz found a crash in $t (see fuzz\artifacts)" }
}
