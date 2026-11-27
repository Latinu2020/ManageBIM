<#
.SYNOPSIS
  Complete ManageBIM backup, ready to move to another computer: the application + all add-ins + per-user
  installer + your settings + the source code. Result: a folder + .zip in Downloads (or -Out).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\make-backup.ps1
#>
param(
    [string]$Out = (Join-Path $env:USERPROFILE 'Downloads'),
    [switch]$NoBuild,
    # without the OpenCascade source archive (~43 MB) in Source\ - only for personal backup, not for distribution (LGPL)
    [switch]$NoOcctSource
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$date = Get-Date -Format 'yyyy-MM-dd'
$B = Join-Path $Out "ManageBIM-backup-$date"
function Step($m) { Write-Host "`n==> $m" -ForegroundColor Cyan }

# ------------------------------------------------------------ 1. the latest version
if (-not $NoBuild) {
    # first the Navisworks add-in: its DLLs end up in installers\payload and are included in ManageBIM-Navisworks-AddIn.exe
    if (Test-Path (Join-Path $root 'navis-addin\build.ps1')) {
        Step 'Building the Navisworks add-in (2022 - 2026)'
        try { & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'navis-addin\build.ps1') -NoInstall } catch { Write-Warning "Navisworks add-in skipped: $_" }
    }
    if (Test-Path (Join-Path $root 'revit-addin\build.ps1')) {
        Step 'Building the Revit add-in (2023 - 2026)'
        try { & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'revit-addin\build.ps1') } catch { Write-Warning "Revit add-in skipped (using the existing DLLs): $_" }
    }
    if (Test-Path (Join-Path $root 'acad-addin\build.ps1')) {
        Step 'Building the AutoCAD / Plant 3D add-in (R24 + R25)'
        try { & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'acad-addin\build.ps1') } catch { Write-Warning "AutoCAD add-in skipped (using the existing DLLs): $_" }
    }
    Step 'Building ManageBIM.exe, the add-in installers and bimc-convert'
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Push-Location $root
        try { & cargo build --release --features addins; if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' } } finally { Pop-Location }
    } else { Write-Warning 'cargo not found - using the existing builds in target\release' }
}
$exe = Join-Path $root 'target\release\ManageBIM.exe'
if (-not (Test-Path $exe)) { throw "Missing $exe - build it first (cargo build --release)." }

# ------------------------------------------------------------ 2. the application package
Step "Assembling $B"
if (Test-Path $B) { Remove-Item $B -Recurse -Force }
$app = Join-Path $B 'ManageBIM'
New-Item -ItemType Directory -Force $app | Out-Null
Copy-Item $exe $app
# bimc-convert: from the build if it exists, otherwise from the original downloaded package
$conv = @((Join-Path $root 'target\release\bimc-convert.exe'), (Join-Path $env:USERPROFILE 'Downloads\ManageBIM-Windows-x64\ManageBIM\bimc-convert.exe')) | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($conv) { Copy-Item $conv $app } else { Write-Warning 'bimc-convert.exe not found - skipped' }
# OpenCascade (native STEP / IGES / BREP): mbocct.dll + the required OCCT DLLs + the LGPL license, placed by build.rs
$occt = Join-Path $root 'target\release\occt'
if (Test-Path (Join-Path $occt 'mbocct.dll')) {
    Copy-Item $occt (Join-Path $app 'occt') -Recurse
    Write-Host ("OpenCascade runtime: {0:N1} MB" -f ((Get-ChildItem $occt -File | Measure-Object Length -Sum).Sum / 1MB))
} else { Write-Warning 'OpenCascade runtime (target\release\occt) missing - STEP / IGES / BREP / FCStd and DWG solids will not open. Run tools\build-occt.ps1, then cargo build --release.' }

# add-ins: .exe installers (per user, no admin), with the DLLs included in them
$add = Join-Path $app 'AddIns'
New-Item -ItemType Directory -Force $add | Out-Null
foreach ($n in 'ManageBIM-Revit-AddIn', 'ManageBIM-AutoCAD-AddIn', 'ManageBIM-Navisworks-AddIn') {
    $f = Join-Path $root "target\release\$n.exe"
    if (Test-Path $f) { Copy-Item $f $add } else { Write-Warning "$n.exe not built - run cargo build --release" }
}
Copy-Item (Join-Path $root 'installers\AddIns-README.txt') (Join-Path $add 'README.txt') -ErrorAction SilentlyContinue
# nwd2bimc.exe: ManageBIM uses it to open .nwd / .nwc / .nwf directly (through Navisworks)
$nbin = Join-Path $root 'navis-addin\bin'
$cli = @((Join-Path $nbin 'nwd2bimc.exe')) + @(Get-ChildItem $nbin -Directory -ErrorAction SilentlyContinue | ForEach-Object { Join-Path $_.FullName 'nwd2bimc.exe' }) | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($cli) {
    $nd = Join-Path $add 'Navisworks'
    New-Item -ItemType Directory -Force $nd | Out-Null
    Copy-Item $cli $nd
    Copy-Item (Join-Path $root 'navis-addin\README.md') $nd
}
Copy-Item (Join-Path $root 'samples') (Join-Path $app 'examples') -Recurse
$docs = Join-Path $app 'docs'
New-Item -ItemType Directory -Force $docs | Out-Null
Copy-Item (Join-Path $root 'README.md'), (Join-Path $root 'BIMC_FORMAT.md'), (Join-Path $root 'CHANGELOG.md'), (Join-Path $root 'THIRD_PARTY.md') $docs
Copy-Item (Join-Path $root 'navis-addin\README.md') (Join-Path $docs 'Navisworks.md')

# ------------------------------------------------------------ 3. installer
Copy-Item (Join-Path $root 'installers\user\install.ps1'), (Join-Path $root 'installers\user\Install-ManageBIM.cmd'), (Join-Path $root 'installers\user\Uninstall-ManageBIM.cmd') $B
# the installer in a single .exe (application + OpenCascade + add-ins), next to the backup
Step 'Building ManageBIM-Setup.exe (single-file installer)'
try { & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'tools\make-setup.ps1') -Out $B } catch { Write-Warning "Setup.exe skipped: $_" }

# ------------------------------------------------------------ 4. your settings
$set = Join-Path $env:APPDATA 'ManageBIM\settings.json'
if (Test-Path $set) {
    New-Item -ItemType Directory -Force (Join-Path $B 'Settings') | Out-Null
    Copy-Item $set (Join-Path $B 'Settings')
}

# ------------------------------------------------------------ 5. the source code (without the build files)
Step 'Copying the source code'
$srcOut = Join-Path $B 'Source\ManageBIM-source'
# third_party\occt is generated by tools\build-occt.ps1 (~ hundreds of MB with the libs) - not part of the source
& robocopy $root $srcOut /E /XD (Join-Path $root 'target') (Join-Path $root 'installers\setup\target') (Join-Path $root 'dist') (Join-Path $root 'third_party\occt')(Join-Path $root 'navis-addin\bin') (Join-Path $root 'navis-addin\refs') (Join-Path $root 'acad-addin\refs') (Join-Path $root 'acad-addin\obj') (Join-Path $root 'revit-addin\refs') (Join-Path $root 'revit-addin\obj') /XF '*.bimc' '*.bimc.log' /NFL /NDL /NJH /NJS /NP | Out-Null
if ($LASTEXITCODE -ge 8) { throw "robocopy failed ($LASTEXITCODE)" }
# the exact OpenCascade source used (LGPL: the library source accompanies the binaries), if downloaded
$occtSrc = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'ManageBIM-dev') -Filter 'occt-*.tar.gz' -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -Last 1
if ($occtSrc -and -not $NoOcctSource) {
    $tp = Join-Path $srcOut 'third_party'
    New-Item -ItemType Directory -Force $tp | Out-Null
    Copy-Item $occtSrc.FullName $tp
}

# ------------------------------------------------------------ 6. instructions
$readme = @"
ManageBIM - backup din $date
==========================================

INSTALARE PE ALT CALCULATOR (fara drepturi de administrator)
  1. Copiaza tot folderul (sau dezarhiveaza .zip-ul) pe noul calculator.
  2. Dublu-click pe  Install-ManageBIM.cmd
     - aplicatia se instaleaza in  %LOCALAPPDATA%\Programs\ManageBIM
     - scurtaturi in Start Menu si pe Desktop
     - .bimc / .bimr se deschid cu ManageBIM; "Open with" pentru .ifc / .nwd / .nwc
     - add-in-urile pentru Revit 2023-2027, AutoCAD 2021+ si Navisworks Manage / Simulate
       (doar pentru programele instalate pe acel calculator)
     - setarile tale (panouri, gruparea arborelui, tema...) daca acolo nu exista deja setari
  3. Dezinstalare: Settings > Apps > ManageBIM, sau  Uninstall-ManageBIM.cmd

CONTINUT
  ManageBIM\            aplicatia gata de rulat (ManageBIM.exe, bimc-convert.exe, AddIns, examples, docs)
  Install-ManageBIM.cmd installer per-user      Uninstall-ManageBIM.cmd
  Settings\             setarile tale ManageBIM
  Source\               tot codul sursa (Rust + add-in-uri Revit / AutoCAD / Navisworks + scripturi)

DESCHIDERE NWD / NWC / NWF / RVT
  Necesita Autodesk Navisworks Manage / Simulate instalat pe calculatorul nou. Installer-ul pune
  automat add-in-ul in Navisworks; dupa asta File > Open in ManageBIM merge direct pe .nwd / .nwc.
  Conversie in lot:  "%LOCALAPPDATA%\Programs\ManageBIM\AddIns\Navisworks\nwd2bimc.exe" <folder>

SMART APP CONTROL / SMARTSCREEN
  Programele nu sunt semnate digital. Installer-ul scoate marcajul "descarcat din internet" de pe
  fisierele copiate; daca Windows tot le blocheaza, solutia corecta e semnarea cu un certificat
  (vezi Source\ManageBIM-source\tools\build-release.ps1).

RECOMPILARE DIN SURSA (optional)
  Rust (rustup.rs) + Visual Studio Build Tools (C++):  cargo build --release
  Add-in Navisworks:  powershell -ExecutionPolicy Bypass -File navis-addin\build.ps1
  Backup nou:         powershell -ExecutionPolicy Bypass -File tools\make-backup.ps1
"@
[IO.File]::WriteAllText((Join-Path $B 'CITESTE-MA.txt'), $readme, (New-Object Text.UTF8Encoding $true))

# ------------------------------------------------------------ 7. archive
Step 'Creating the .zip'
$zip = "$B.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $B '*') -DestinationPath $zip -CompressionLevel Optimal
$size = (Get-ChildItem $B -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ("`nBackup: {0}  ({1:N1} MB)`nZip:    {2}  ({3:N1} MB)" -f $B, ($size / 1MB), $zip, ((Get-Item $zip).Length / 1MB))
