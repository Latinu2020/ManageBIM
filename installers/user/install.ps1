<#
.SYNOPSIS
  Instalare ManageBIM pentru utilizatorul curent (fara drepturi de administrator).

.DESCRIPTION
  Din folderul pachetului (langa acest script trebuie sa fie folderul "ManageBIM"):
    - aplicatia      -> %LOCALAPPDATA%\Programs\ManageBIM
    - scurtaturi     -> Start Menu + Desktop
    - fisiere        -> .bimc / .bimr se deschid cu ManageBIM; "Open with" pentru .ifc .nwd .nwc
    - Apps & features-> intrare "ManageBIM" cu Uninstall
    - add-in-uri     -> Revit 2023-2027, AutoCAD 2021+, Navisworks Manage / Simulate (doar ce e instalat)
    - setari         -> restaurate din Settings\settings.json (daca nu exista deja setari pe acest PC)
  -Uninstall le sterge pe toate (setarile utilizatorului raman).
  -NoAddins instaleaza doar aplicatia. -TestRoot <folder> simuleaza totul intr-un folder (fara registru).
#>
param(
    [switch]$Uninstall,
    [switch]$NoAddins,
    # lista add-in-urilor de instalat (revit,acad,navis); gol = toate cele gasite
    [string]$Addins = '',
    [switch]$NoDesktop,
    [string]$TestRoot = ''
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$version = '0.27.14'

# --- locatii (redirectionate in modul test)
if ($TestRoot) {
    New-Item -ItemType Directory -Force $TestRoot | Out-Null
    $LocalApp = Join-Path $TestRoot 'LocalAppData'
    $RoamApp = Join-Path $TestRoot 'AppData'
    $Desktop = Join-Path $TestRoot 'Desktop'
    $StartMenu = Join-Path $TestRoot 'StartMenu'
} else {
    $LocalApp = $env:LOCALAPPDATA
    $RoamApp = $env:APPDATA
    $Desktop = [Environment]::GetFolderPath('Desktop')
    $StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
}
$AppDir = Join-Path $LocalApp 'Programs\ManageBIM'
$Exe = Join-Path $AppDir 'ManageBIM.exe'
$PF = $env:ProgramFiles

function Say($m) { Write-Host $m }
function Ok($m) { Write-Host "  [OK] $m" -ForegroundColor Green }

function Set-Reg($key, $name, $value) {
    if ($TestRoot) { return }
    $k = "HKCU:\$key"
    if (-not (Test-Path $k)) { New-Item -Path $k -Force | Out-Null }
    if ($name) { New-ItemProperty -Path $k -Name $name -Value $value -PropertyType String -Force | Out-Null }
    else { Set-Item -Path $k -Value $value }
}
function Remove-Reg($key) { if (-not $TestRoot -and (Test-Path "HKCU:\$key")) { Remove-Item "HKCU:\$key" -Recurse -Force } }

function New-Shortcut($lnk, $target) {
    New-Item -ItemType Directory -Force (Split-Path $lnk) | Out-Null
    $sh = New-Object -ComObject WScript.Shell
    $s = $sh.CreateShortcut($lnk)
    $s.TargetPath = $target
    $s.WorkingDirectory = Split-Path $target
    $s.IconLocation = "$target,0"
    $s.Description = 'ManageBIM - federated BIM review & coordination'
    $s.Save()
}

# --- installerele add-in-urilor (.exe), din aplicatia instalata sau din pachet
function AddinExes {
    $dirs = @((Join-Path $AppDir 'AddIns'), (Join-Path $here 'ManageBIM\AddIns'))
    $want = @($Addins.ToLower().Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    foreach ($n in 'ManageBIM-Revit-AddIn.exe', 'ManageBIM-AutoCAD-AddIn.exe', 'ManageBIM-Navisworks-AddIn.exe') {
        if ($want.Count -gt 0) {
            $key = if ($n -like '*Revit*') { 'revit' } elseif ($n -like '*AutoCAD*') { 'acad' } else { 'navis' }
            if ($want -notcontains $key) { continue }
        }
        $f = $dirs | ForEach-Object { Join-Path $_ $n } | Where-Object { Test-Path $_ } | Select-Object -First 1
        if ($f) { $f }
    }
}
function Run-AddinExe($exe, $mode) {
    $tmp = [IO.Path]::GetTempFileName()
    $old = $env:APPDATA
    try {
        $env:APPDATA = $RoamApp   # installerele scriu in %APPDATA% (redirectionat in modul test)
        Start-Process $exe -ArgumentList $mode -Wait -NoNewWindow -RedirectStandardOutput $tmp -RedirectStandardError "$tmp.err" | Out-Null
        @(Get-Content $tmp, "$tmp.err" -ErrorAction SilentlyContinue | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    } finally {
        $env:APPDATA = $old
        Remove-Item $tmp, "$tmp.err" -ErrorAction SilentlyContinue
    }
}

# --- add-in-uri: unde se instaleaza
function Get-RevitYears { 2023..2027 | Where-Object { Test-Path (Join-Path $PF "Autodesk\Revit $_") } }
function Get-AcadInstalled { [bool](Get-ChildItem (Join-Path $PF 'Autodesk') -Directory -Filter 'AutoCAD*' -ErrorAction SilentlyContinue) }
function Revit-Dll($year) { if ($year -ge 2026) { 2026 } else { $year } }

if (Get-Process ManageBIM -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$AppDir*" }) {
    Write-Warning 'Close ManageBIM first.'
    exit 1
}

# ======================================================================= dezinstalare
if ($Uninstall) {
    Say 'Uninstalling ManageBIM...'
    # intai prin installerele .exe (stiu exact ce au pus); apoi curatenie pentru instalari mai vechi
    foreach ($a in AddinExes) {
        foreach ($line in (Run-AddinExe $a '--uninstall')) { if ($line -match ': removed') { Ok $line } }
    }
    foreach ($y in 2023..2027) {
        $d = Join-Path $RoamApp "Autodesk\Revit\Addins\$y"
        if (Test-Path (Join-Path $d 'ManageBIM.addin')) {
            Remove-Item (Join-Path $d 'ManageBIM.addin') -Force
            Remove-Item (Join-Path $d 'ManageBIM') -Recurse -Force -ErrorAction SilentlyContinue
            Ok "Revit $y add-in removed"
        }
    }
    $bundle = Join-Path $RoamApp 'Autodesk\ApplicationPlugins\ManageBIM.bundle'
    if (Test-Path $bundle) { Remove-Item $bundle -Recurse -Force; Ok 'AutoCAD add-in removed' }
    $navisDirs = @(Get-ChildItem (Join-Path $RoamApp 'Autodesk') -Directory -Filter 'Navisworks*' -ErrorAction SilentlyContinue) + @(Get-ChildItem $RoamApp -Directory -Filter 'Autodesk Navisworks*' -ErrorAction SilentlyContinue)
    foreach ($p in $navisDirs) {
        $pl = Join-Path $p.FullName 'Plugins\ManageBIM.NwExporter'
        if (Test-Path $pl) { Remove-Item $pl -Recurse -Force; Ok "$($p.Name) add-in removed" }
    }
    Remove-Item (Join-Path $Desktop 'ManageBIM.lnk'), (Join-Path $StartMenu 'ManageBIM.lnk') -Force -ErrorAction SilentlyContinue
    foreach ($k in 'Software\Classes\ManageBIM.Cache', 'Software\Classes\ManageBIM.Project', 'Software\Classes\ManageBIM.Ifc', 'Software\Microsoft\Windows\CurrentVersion\Uninstall\ManageBIM') { Remove-Reg $k }
    foreach ($ext in '.bimc', '.bimr', '.bimd') {
        if (-not $TestRoot -and (Get-ItemProperty "HKCU:\Software\Classes\$ext" -ErrorAction SilentlyContinue).'(default)' -like 'ManageBIM.*') { Remove-Reg "Software\Classes\$ext" }
    }
    foreach ($ext in '.ifc', '.nwd', '.nwc') {
        if (-not $TestRoot) { Remove-ItemProperty "HKCU:\Software\Classes\$ext\OpenWithProgids" -Name 'ManageBIM.Ifc' -ErrorAction SilentlyContinue }
    }
    # scriptul ruleaza chiar din folderul aplicatiei: il stergem dupa iesire
    if (Test-Path $AppDir) {
        if ($here -like "$AppDir*") {
            Start-Process cmd.exe -ArgumentList "/c timeout /t 2 >nul & rmdir /s /q `"$AppDir`"" -WindowStyle Hidden
        } else {
            Remove-Item $AppDir -Recurse -Force
        }
    }
    Ok 'ManageBIM removed (your settings in %APPDATA%\ManageBIM were kept)'
    exit 0
}

# ======================================================================= instalare
$pkg = Join-Path $here 'ManageBIM'
if (-not (Test-Path (Join-Path $pkg 'ManageBIM.exe'))) { throw "Folder 'ManageBIM' with ManageBIM.exe not found next to this script." }
Say "Installing ManageBIM $version for $env:USERNAME..."

# 1) aplicatia
if (Test-Path $AppDir) { Remove-Item $AppDir -Recurse -Force }
New-Item -ItemType Directory -Force $AppDir | Out-Null
Copy-Item (Join-Path $pkg '*') $AppDir -Recurse -Force
Copy-Item (Join-Path $here 'install.ps1') (Join-Path $AppDir 'uninstall.ps1') -Force
# fisierele copiate din arhiva descarcata poarta marcajul "din internet": il scoatem (sunt fisierele tale)
Get-ChildItem $AppDir -Recurse -File | Unblock-File
Ok "Application -> $AppDir"

# 2) scurtaturi
New-Shortcut (Join-Path $StartMenu 'ManageBIM.lnk') $Exe
if ($NoDesktop) {
    Remove-Item (Join-Path $Desktop 'ManageBIM.lnk') -Force -ErrorAction SilentlyContinue
    Ok 'Shortcut: Start Menu'
} else {
    New-Shortcut (Join-Path $Desktop 'ManageBIM.lnk') $Exe
    Ok 'Shortcuts: Start Menu + Desktop'
}

# 3) fisiere asociate (HKCU, doar utilizatorul curent)
$open = "`"$Exe`" `"%1`""
foreach ($t in @(@('.bimc', 'ManageBIM.Cache', 'ManageBIM model cache'), @('.bimr', 'ManageBIM.Project', 'ManageBIM project'), @('.bimd', 'ManageBIM.Published', 'ManageBIM published project'), @('', 'ManageBIM.Ifc', 'BIM model'))) {
    if ($t[0]) { Set-Reg "Software\Classes\$($t[0])" $null $t[1] }
    Set-Reg "Software\Classes\$($t[1])" $null $t[2]
    Set-Reg "Software\Classes\$($t[1])\DefaultIcon" $null "`"$Exe`",0"
    Set-Reg "Software\Classes\$($t[1])\shell\open\command" $null $open
}
foreach ($ext in '.ifc', '.nwd', '.nwc') { Set-Reg "Software\Classes\$ext\OpenWithProgids" 'ManageBIM.Ifc' '' }
Ok '.bimc / .bimr / .bimd open with ManageBIM; "Open with" for .ifc .nwd .nwc'

# 4) Apps & features
$u = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\ManageBIM'
Set-Reg $u 'DisplayName' 'ManageBIM'
Set-Reg $u 'DisplayVersion' $version
Set-Reg $u 'Publisher' 'ManageBIM'
Set-Reg $u 'InstallLocation' $AppDir
Set-Reg $u 'DisplayIcon' "`"$Exe`",0"
Set-Reg $u 'UninstallString' "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$AppDir\uninstall.ps1`" -Uninstall"
if (-not $TestRoot) { New-ItemProperty "HKCU:\$u" -Name NoModify -Value 1 -PropertyType DWord -Force | Out-Null; New-ItemProperty "HKCU:\$u" -Name NoRepair -Value 1 -PropertyType DWord -Force | Out-Null }
Ok 'Listed in Settings > Apps (Uninstall available there)'

# 5) setari (doar daca pe acest PC nu exista deja)
$set = Join-Path $here 'Settings\settings.json'
$dst = Join-Path $RoamApp 'ManageBIM\settings.json'
if ((Test-Path $set) -and -not (Test-Path $dst)) {
    New-Item -ItemType Directory -Force (Split-Path $dst) | Out-Null
    $j = Get-Content $set -Raw | ConvertFrom-Json
    # caile convertorilor externi tin de PC-ul vechi: ManageBIM le cauta din nou la pornire
    if ($j.PSObject.Properties.Name -contains 'tools') { $j.PSObject.Properties.Remove('tools') }
    if ($j.PSObject.Properties.Name -contains 'recent') { $j.recent = @() }
    [IO.File]::WriteAllText($dst, ($j | ConvertTo-Json -Depth 30), (New-Object Text.UTF8Encoding $false))
    Ok 'Your settings restored (panels, tree grouping, theme...)'
}

# 6) add-in-uri: installerele .exe din AddIns (fiecare instaleaza in versiunile gasite, fara admin)
if (-not $NoAddins) {
    foreach ($a in AddinExes) {
        foreach ($line in (Run-AddinExe $a '--install')) { if ($line -match ': installed') { Ok $line } elseif ($line) { Write-Warning $line } }
    }
}

Say ''
Say "Done. Start ManageBIM from the Start Menu or the Desktop shortcut."
Say "Uninstall: Settings > Apps > ManageBIM, or run Uninstall-ManageBIM.cmd."
