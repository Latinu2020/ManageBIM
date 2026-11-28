<#
.SYNOPSIS
  One-command local release verification: tests -> clippy/rustfmt gate -> release build (with add-ins) ->
  Windows setup -> interactive Ray Trace smoke test -> SHA256 manifest. Nothing is published.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\verify-release.ps1
  powershell -ExecutionPolicy Bypass -File tools\verify-release.ps1 -SkipTests -Out D:\Kit
#>
[CmdletBinding()]
param(
    [string]$Out = (Join-Path $env:USERPROFILE 'Downloads'),
    [switch]$SkipTests,
    [switch]$SkipInstaller
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
function Step($m) { Write-Host "`n==> $m" -ForegroundColor Cyan }
function Run([string]$exe, [string[]]$args) {
    Write-Host "> $exe $($args -join ' ')"
    & $exe @args
    if ($LASTEXITCODE -ne 0) { throw "$exe failed (exit $LASTEXITCODE)" }
}

if (-not $SkipTests) {
    Step 'cargo test --release'
    Run 'cargo' @('test', '--release')
}

Step 'clippy / rustfmt gate'
$bash = (Get-Command bash.exe -ErrorAction SilentlyContinue).Source
if (-not $bash) { $bash = Join-Path ${env:ProgramFiles} 'Git\bin\bash.exe' }
if (Test-Path $bash) {
    Run $bash @('packaging/ci/quality.sh', '')
} else {
    Write-Warning 'bash not found - quality.sh skipped (clippy baseline not checked)'
}

Step 'release build (application + three add-in installers)'
Run 'cargo' @('build', '--release', '--features', 'addins', '--bin', 'ManageBIM', '--bin', 'bimc-convert', '--bin', 'ManageBIM-Revit-AddIn', '--bin', 'ManageBIM-AutoCAD-AddIn', '--bin', 'ManageBIM-Navisworks-AddIn')

if (-not $SkipInstaller) {
    Step 'ManageBIM-Setup.exe'
    Run 'powershell' @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'tools\make-setup.ps1', '-Out', $Out)
}

Step 'interactive Ray Trace smoke test'
Run 'powershell' @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'tools\test-raytrace.ps1', '-Out', (Join-Path $env:TEMP 'bim-verify-raytrace'))

Step "SHA256 manifest for $Out"
$files = Get-ChildItem $Out -Filter "ManageBIM-Setup-$version.exe" -ErrorAction SilentlyContinue
if ($files) {
    $lines = foreach ($f in $files) { "{0}  {1}" -f (Get-FileHash $f.FullName -Algorithm SHA256).Hash, $f.Name }
    $sum = Join-Path $Out "SHA256SUMS-$version.txt"
    [IO.File]::WriteAllLines($sum, $lines)
    Get-Content $sum
} else {
    Write-Warning "No installer $Out\ManageBIM-Setup-$version.exe (skip installer with -SkipInstaller?)"
}

Step 'Done - nothing was published'
