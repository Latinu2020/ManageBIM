<#
.SYNOPSIS
  Builds ManageBIM-Setup.exe: ONE single file with the application, OpenCascade, the samples and the add-ins
  (Revit, AutoCAD / Plant 3D, Navisworks). Per-user install, no administrator rights.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\make-setup.ps1            # after cargo build --release
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

# ------------------------------------------------ the package (the same structure as the backup: install.ps1 + ManageBIM\)
if (Test-Path $pkg) { Remove-Item $pkg -Recurse -Force }
$app = Join-Path $pkg 'ManageBIM'
New-Item -ItemType Directory -Force $app, (Join-Path $app 'AddIns'), (Join-Path $app 'docs') | Out-Null
# install.ps1 with this version: Settings > Apps and the wizard ("installed: x.y.z") read it
$ver = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$ps = (Get-Content (Join-Path $root 'installers\user\install.ps1') -Raw) -replace "(?m)^\`$version = '[^']*'", "`$version = '$ver'"
[IO.File]::WriteAllText((Join-Path $pkg 'install.ps1'), $ps, (New-Object Text.UTF8Encoding $true))
Copy-Item (Join-Path $rel 'ManageBIM.exe'), (Join-Path $rel 'bimc-convert.exe') $app
if (Test-Path (Join-Path $rel 'occt')) { Copy-Item (Join-Path $rel 'occt') (Join-Path $app 'occt') -Recurse }
# Intel Open Image Denoise (Render › Denoise) - tools\get-oidn.ps1 downloads it into third_party\oidn
$oidn = Join-Path $root 'third_party\oidn'
# the default PBR library (ambientCG scanned materials, CC0) - cargo run --release --bin fetch_pro_library
$pbr = Join-Path $root 'third_party\pbr-library'
if (Test-Path (Join-Path $pbr 'library.bimmat')) { Copy-Item $pbr (Join-Path $app 'materials') -Recurse } else { Write-Warning 'Default PBR library missing (cargo run --release --bin fetch_pro_library) - only the simple built-in materials will be available.' }
if (Test-Path (Join-Path $oidn 'OpenImageDenoise.dll')) { Copy-Item $oidn (Join-Path $app 'oidn') -Recurse } else { Write-Warning 'Open Image Denoise missing (tools\get-oidn.ps1) - Render denoise will be unavailable.' }
# Visual C++ runtime next to ManageBIM.exe (app-local, allowed by Microsoft's redistribution terms): Open Image
# Denoise / TBB need it; the Rust programs carry their own C runtime (.cargo\config.toml, crt-static), so a clean
# Windows needs no Visual C++ Redistributable.
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$crt = $null
if (Test-Path $vswhere) {
    foreach ($vs in (& $vswhere -all -products * -property installationPath)) {
        $c = Get-ChildItem (Join-Path $vs 'VC\Redist\MSVC') -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending |
            ForEach-Object { Get-ChildItem (Join-Path $_.FullName 'x64') -Directory -Filter 'Microsoft.VC14*.CRT' -ErrorAction SilentlyContinue } | Select-Object -First 1
        if ($c) { $crt = $c.FullName; break }
    }
}
if ($crt) {
    Get-ChildItem $crt -Filter '*.dll' | Where-Object { $_.Name -match '^(vcruntime140(_1)?|msvcp140(_1|_2|_atomic_wait|_codecvt_ids)?|concrt140)\.dll$' } | Copy-Item -Destination $app
} else { throw 'Visual C++ runtime (VC\Redist\MSVC\...\Microsoft.VC14x.CRT) not found - install the Visual Studio Build Tools with C++.' }
# Mesa software OpenGL (llvmpipe) for PCs without OpenGL 3.3 - tools\get-mesa.ps1 downloads it into third_party\mesa
$mesa = Join-Path $root 'third_party\mesa'
if (Test-Path (Join-Path $mesa 'opengl32.dll')) { Copy-Item $mesa (Join-Path $app 'mesa') -Recurse } else { Write-Warning 'Mesa missing (tools\get-mesa.ps1) - PCs without OpenGL 3.3 will get a message instead of the 3D view.' }
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
# the example scripts (Rhai) - the console installs them from here into the scripts folder
$exScripts = Join-Path $root 'examples\scripts'
if (Test-Path $exScripts) {
    New-Item -ItemType Directory -Force (Join-Path $app 'examples\scripts') | Out-Null
    Copy-Item (Join-Path $exScripts '*.rhai') (Join-Path $app 'examples\scripts') -Force
}
Copy-Item (Join-Path $root 'README.md'), (Join-Path $root 'BIMC_FORMAT.md'), (Join-Path $root 'CHANGELOG.md'), (Join-Path $root 'THIRD_PARTY.md') (Join-Path $app 'docs')
$size = (Get-ChildItem $pkg -Recurse -File | Measure-Object Length -Sum).Sum

# ------------------------------------------------ the installer (compressed payload embedded in the exe)
Write-Host ("Package: {0:N1} MB -> compressing into ManageBIM-Setup.exe" -f ($size / 1MB))
$env:MB_PACKAGE_DIR = $pkg
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
& cargo build --release --manifest-path (Join-Path $root 'installers\setup\Cargo.toml') --target-dir (Join-Path $root 'target\setup')
if ($LASTEXITCODE -ne 0) { throw 'Setup build failed' }
$dst = Join-Path $Out "ManageBIM-Setup-$ver.exe"
Copy-Item (Join-Path $root 'target\setup\release\ManageBIM-Setup.exe') $dst -Force
Write-Host ("`nSetup: {0}  ({1:N1} MB)" -f $dst, ((Get-Item $dst).Length / 1MB)) -ForegroundColor Green
