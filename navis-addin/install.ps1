<#
  Instaleaza add-in-ul ManageBIM (deja compilat, din acest folder) in toate versiunile Navisworks
  Manage / Simulate gasite, fiecare cu DLL-ul compilat pentru versiunea ei (<an>\ManageBIM.NwExporter.dll).
  Fara drepturi de administrator (doar pentru utilizatorul curent). -Uninstall il sterge.
#>
param([switch]$Uninstall, [string]$AppData = $env:APPDATA)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
if (-not $Uninstall -and (Get-Process Roamer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 })) { Write-Warning 'Close Navisworks first.'; exit 1 }
$n = 0
foreach ($d in Get-ChildItem (Join-Path $env:ProgramFiles 'Autodesk') -Directory -Filter 'Navisworks*' -ErrorAction SilentlyContinue) {
    if (-not (Test-Path (Join-Path $d.FullName 'Autodesk.Navisworks.Api.dll'))) { continue }
    if ($d.Name -notmatch '^Navisworks (\w+) (\d{4})$') { continue }
    $ed, $year = $matches[1], [int]$matches[2]
    # 2024+: %APPDATA%\Autodesk\Navisworks <ed> <an>; versiunile vechi: %APPDATA%\Autodesk Navisworks <ed> <an>
    $new = Join-Path $AppData "Autodesk\Navisworks $ed $year"
    $old = Join-Path $AppData "Autodesk Navisworks $ed $year"
    $base = if ((Test-Path $new) -or $year -ge 2024) { $new } else { $old }
    $plug = Join-Path $base 'Plugins\ManageBIM.NwExporter'
    if ($Uninstall) {
        foreach ($p in (Join-Path $new 'Plugins\ManageBIM.NwExporter'), (Join-Path $old 'Plugins\ManageBIM.NwExporter')) {
            if (Test-Path $p) { Remove-Item $p -Recurse -Force; Write-Host "Removed from Navisworks $ed $year" }
        }
        continue
    }
    $dll = Join-Path $here "$year\ManageBIM.NwExporter.dll"
    if (-not (Test-Path $dll)) { Write-Warning "Navisworks $ed $year found, but there is no add-in built for $year in this package."; continue }
    New-Item -ItemType Directory -Force $plug | Out-Null
    Copy-Item $dll $plug -Force
    Get-ChildItem $plug -File | Unblock-File
    Write-Host "Installed in Navisworks $ed $year  ->  Tool add-ins > Export to ManageBIM (.bimc)"
    $n++
}
if (-not $Uninstall -and $n -eq 0) { Write-Warning 'No Navisworks Manage / Simulate installation found.' }
