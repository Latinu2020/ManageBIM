<#
.SYNOPSIS
  Construieste ManageBIM-Setup.exe: UN singur fisier cu aplicatia, OpenCascade, exemplele si add-in-urile
  (Revit, AutoCAD / Plant 3D, Navisworks). Instalare per utilizator, fara admin.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\make-setup.ps1            # dupa cargo build --release
  powershell -ExecutionPolicy Bypass -File tools\make-setup.ps1 -Out D:\Kit
#>
param(
    [string]$Out = (Join-Path $env:USERPROFILE 'Downloads')
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $Out | Out-Null
$root = Split-Path -Parent $PSScriptRoot
$rel = Join-Path $root 'target\release'
$pkg = Join-Path $root 'target\setup-package'
if (-not (Test-Path (Join-Path $rel 'ManageBIM.exe'))) { throw 'Build first: cargo build --release' }
if (-not (Test-Path (Join-Path $rel 'occt\mbocct.dll'))) { Write-Warning 'OpenCascade runtime missing (tools\build-occt.ps1) - STEP / IGES / FCStd / DWG solids will not open.' }

# ------------------------------------------------ pachetul (aceeasi structura ca backup-ul: install.ps1 + ManageBIM\)
if (Test-Path $pkg) { Remove-Item $pkg -Recurse -Force }
$app = Join-Path $pkg 'ManageBIM'
New-Item -ItemType Directory -Force $app, (Join-Path $app 'AddIns'), (Join-Path $app 'docs') | Out-Null
Copy-Item (Join-Path $root 'installers\user\install.ps1') $pkg
Copy-Item (Join-Path $rel 'ManageBIM.exe'), (Join-Path $rel 'bimc-convert.exe') $app
if (Test-Path (Join-Path $rel 'occt')) { Copy-Item (Join-Path $rel 'occt') (Join-Path $app 'occt') -Recurse }
# Intel Open Image Denoise (Render › Denoise) - tools\get-oidn.ps1 il descarca in third_party\oidn
$oidn = Join-Path $root 'third_party\oidn'
# biblioteca PBR implicita (materiale scanate ambientCG, CC0) - cargo run --release --bin fetch_pro_library
$pbr = Join-Path $root 'third_party\pbr-library'
if (Test-Path (Join-Path $pbr 'library.bimmat')) { Copy-Item $pbr (Join-Path $app 'materials') -Recurse } else { Write-Warning 'Default PBR library missing (cargo run --release --bin fetch_pro_library) - only the simple built-in materials will be available.' }
if (Test-Path (Join-Path $oidn 'OpenImageDenoise.dll')) { Copy-Item $oidn (Join-Path $app 'oidn') -Recurse } else { Write-Warning 'Open Image Denoise missing (tools\get-oidn.ps1) - Render denoise will be unavailable.' }
foreach ($n in 'ManageBIM-Revit-AddIn', 'ManageBIM-AutoCAD-AddIn', 'ManageBIM-Navisworks-AddIn') {
    $f = Join-Path $rel "$n.exe"
    if (Test-Path $f) { Copy-Item $f (Join-Path $app 'AddIns') } else { Write-Warning "$n.exe missing" }
}
Copy-Item (Join-Path $root 'installers\AddIns-README.txt') (Join-Path $app 'AddIns\README.txt') -ErrorAction SilentlyContinue
$nbin = Join-Path $root 'navis-addin\bin\nwd2bimc.exe'
if (Test-Path $nbin) {
    $nd = Join-Path $app 'AddIns\Navisworks'
    New-Item -ItemType Directory -Force $nd | Out-Null
    Copy-Item $nbin $nd
    Copy-Item (Join-Path $root 'navis-addin\README.md') $nd -ErrorAction SilentlyContinue
}
if (Test-Path (Join-Path $root 'samples')) { Copy-Item (Join-Path $root 'samples') (Join-Path $app 'examples') -Recurse }
# scripturile exemplu (Rhai) - consola le instaleaza de aici in folderul de scripturi
$exScripts = Join-Path $root 'examples\scripts'
if (Test-Path $exScripts) {
    New-Item -ItemType Directory -Force (Join-Path $app 'examples\scripts') | Out-Null
    Copy-Item (Join-Path $exScripts '*.rhai') (Join-Path $app 'examples\scripts') -Force
}
Copy-Item (Join-Path $root 'README.md'), (Join-Path $root 'BIMC_FORMAT.md'), (Join-Path $root 'CHANGELOG.md'), (Join-Path $root 'THIRD_PARTY.md') (Join-Path $app 'docs')
$size = (Get-ChildItem $pkg -Recurse -File | Measure-Object Length -Sum).Sum

# ------------------------------------------------ installer-ul (payload comprimat inclus in exe)
Write-Host ("Package: {0:N1} MB -> compressing into ManageBIM-Setup.exe" -f ($size / 1MB))
$env:MB_PACKAGE_DIR = $pkg
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
& cargo build --release --manifest-path (Join-Path $root 'installers\setup\Cargo.toml') --target-dir (Join-Path $root 'target\setup')
if ($LASTEXITCODE -ne 0) { throw 'Setup build failed' }
$ver = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$dst = Join-Path $Out "ManageBIM-Setup-$ver.exe"
Copy-Item (Join-Path $root 'target\setup\release\ManageBIM-Setup.exe') $dst -Force
Write-Host ("`nSetup: {0}  ({1:N1} MB)" -f $dst, ((Get-Item $dst).Length / 1MB)) -ForegroundColor Green
