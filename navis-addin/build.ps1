<#
.SYNOPSIS
  Compileaza exportatorul ManageBIM pentru Navisworks 2022 - 2026 si il instaleaza in versiunile gasite
  pe acest PC (fara drepturi de administrator).

.DESCRIPTION
  - bin\<an>\ManageBIM.NwExporter.dll  - cate unul pentru fiecare versiune (API-ul Navisworks are alt numar de
    versiune in fiecare an). Referintele vin din Navisworks-ul instalat sau, pentru celelalte versiuni, din
    pachetele NuGet "Chuongmep.Navis.Api.Autodesk.Navisworks.Api" (doar referinte, descarcate o data in refs\).
  - bin\nwd2bimc.exe (+ o copie in fiecare bin\<an>\) - conversie din linia de comanda / folosit de ManageBIM.
  - instalare: %APPDATA%\Autodesk\Navisworks <Manage|Simulate> <an>\Plugins\ManageBIM.NwExporter\ (2024+)
               %APPDATA%\Autodesk Navisworks <Manage|Simulate> <an>\Plugins\ManageBIM.NwExporter\ (2023 si mai vechi)
  Necesita compilatorul C# (Visual Studio sau Build Tools 2019+). Navisworks nu e necesar pentru compilare.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File navis-addin\build.ps1
  powershell -ExecutionPolicy Bypass -File navis-addin\build.ps1 -Years 2025 -NoInstall
#>
param(
    [int[]]$Years = @(2022, 2023, 2024, 2025, 2026),
    [switch]$NoInstall
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$pf = $env:ProgramFiles
# ultima versiune NuGet de referinte pentru fiecare an
$nugetVer = @{ 2020 = '2020.0.4'; 2021 = '2021.0.2'; 2022 = '2022.0.6'; 2023 = '2023.0.7'; 2024 = '2024.0.0'; 2025 = '2025.0.0'; 2026 = '2026.0.0' }

# --- compilatorul C# (Roslyn, C# 7+)
$csc = $null
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path $vswhere) {
    foreach ($vs in (& $vswhere -all -products * -property installationPath)) {
        $c = Join-Path $vs 'MSBuild\Current\Bin\Roslyn\csc.exe'
        if (Test-Path $c) { $csc = $c; break }
    }
}
if (-not $csc) {
    # fallback: .NET SDK instalat per-utilizator (Visual Studio fara workload C#)
    $sdkRoot = Join-Path $env:LOCALAPPDATA 'Microsoft\dotnet\sdk'
    $sdk = Get-ChildItem $sdkRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
    if ($sdk) {
        $dll = Join-Path $sdk.FullName 'Roslyn\bincore\csc.dll'
        $dn = Join-Path (Split-Path $sdkRoot -Parent) 'dotnet.exe'
        if ((Test-Path $dll) -and (Test-Path $dn)) {
            $shim = Join-Path $env:TEMP 'managebim-csc.cmd'
            "@echo off`r`n`"$dn`" `"$dll`" %*`r`n" | Set-Content -Encoding ascii $shim
            $csc = $shim
        }
    }
}
if (-not $csc) { throw 'C# compiler not found. Install "Build Tools for Visual Studio" (or Visual Studio) 2019 or newer, or a per-user .NET SDK.' }
Write-Host "csc: $csc"

function Get-Installed {
    $r = @()
    foreach ($d in Get-ChildItem (Join-Path $pf 'Autodesk') -Directory -Filter 'Navisworks*' -ErrorAction SilentlyContinue) {
        if ($d.Name -match '^Navisworks (\w+) (\d{4})$' -and (Test-Path (Join-Path $d.FullName 'Autodesk.Navisworks.Api.dll'))) {
            $r += [pscustomobject]@{ Edition = $matches[1]; Year = [int]$matches[2]; Dir = $d.FullName }
        }
    }
    $r
}
$installed = @(Get-Installed)

# Autodesk.Navisworks.Api.dll pentru un an: din instalare sau din NuGet (descarcat o data)
function Get-ApiRef([int]$year) {
    $inst = $installed | Where-Object { $_.Year -eq $year } | Select-Object -First 1
    if ($inst) { return (Join-Path $inst.Dir 'Autodesk.Navisworks.Api.dll') }
    $v = $nugetVer[$year]
    if (-not $v) { return $null }
    $dir = Join-Path $here "refs\$year"
    $dll = Get-ChildItem $dir -Recurse -Filter 'Autodesk.Navisworks.Api.dll' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($dll) { return $dll.FullName }
    $id = 'chuongmep.navis.api.autodesk.navisworks.api'
    New-Item -ItemType Directory -Force $dir | Out-Null
    $zip = Join-Path $dir "$id.$v.zip"
    [Net.ServicePointManager]::SecurityProtocol = 'Tls12'
    Write-Host "  downloading Navisworks $year API references (NuGet $id $v)..."
    Invoke-WebRequest "https://api.nuget.org/v3-flatcontainer/$id/$v/$id.$v.nupkg" -OutFile $zip -UseBasicParsing
    Expand-Archive $zip $dir -Force
    Remove-Item $zip
    $dll = Get-ChildItem $dir -Recurse -Filter 'Autodesk.Navisworks.Api.dll' | Select-Object -First 1
    if (-not $dll) { throw "Autodesk.Navisworks.Api.dll not found in the NuGet package for $year" }
    return $dll.FullName
}

$fw = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
$common = @('/nologo', '/langversion:latest', '/optimize+', '/platform:x64', '/nowarn:1701,1702', "/r:$fw\mscorlib.dll", "/r:$fw\System.dll", "/r:$fw\System.Core.dll")
$srcs = @('BimcWriter.cs', 'BimcStream.cs', 'NwCom.cs', 'NwExporter.cs', 'ExportPlugin.cs', 'ManageBimLink.cs', 'ManageBimIssues.cs', 'IssuesPlugin.cs', 'SyncPlugin.cs', 'CoordinationPlugin.cs', 'HubPlugin.cs', 'WalkPlugin.cs') | ForEach-Object { Join-Path $here $_ }

# --- add-in: cate unul pentru fiecare an
$built = @{}
foreach ($y in $Years) {
    try { $api = Get-ApiRef $y } catch { Write-Warning "Navisworks $y skipped: $_"; continue }
    if (-not $api) { Write-Warning "Navisworks $y skipped: no API references"; continue }
    $out = Join-Path $here "bin\$y"
    New-Item -ItemType Directory -Force $out | Out-Null
    $dll = Join-Path $out 'ManageBIM.NwExporter.dll'
    & $csc @common '/target:library' "/out:$dll" "/r:$fw\System.Windows.Forms.dll" "/r:$fw\System.Drawing.dll" "/r:$fw\Microsoft.CSharp.dll" "/r:$api" @srcs
    if ($LASTEXITCODE -ne 0) { throw "Add-in build failed for Navisworks $y." }
    $built[$y] = $dll
    # copia pentru installerul ManageBIM-Navisworks-AddIn.exe (DLL-urile sunt incluse in el la compilare)
    $pay = Join-Path (Split-Path -Parent $here) "installers\payload\navisworks\$y"
    New-Item -ItemType Directory -Force $pay | Out-Null
    Copy-Item $dll $pay -Force
    Write-Host "Built: Navisworks $y -> $dll"
}

# --- nwd2bimc.exe (unul singur: Automation se incarca la rulare din versiunea aleasa)
$autoRef = $installed | ForEach-Object { Join-Path $_.Dir 'Autodesk.Navisworks.Automation.dll' } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $autoRef) {
    $dir = Join-Path $here 'refs\automation'
    $autoRef = Get-ChildItem $dir -Recurse -Filter 'Autodesk.Navisworks.Automation.dll' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
    if (-not $autoRef) {
        $id = 'chuongmep.navis.api.autodesk.navisworks.automation'; $v = '2023.0.5'
        New-Item -ItemType Directory -Force $dir | Out-Null
        $zip = Join-Path $dir "$id.zip"
        [Net.ServicePointManager]::SecurityProtocol = 'Tls12'
        Invoke-WebRequest "https://api.nuget.org/v3-flatcontainer/$id/$v/$id.$v.nupkg" -OutFile $zip -UseBasicParsing
        Expand-Archive $zip $dir -Force; Remove-Item $zip
        $autoRef = Get-ChildItem $dir -Recurse -Filter 'Autodesk.Navisworks.Automation.dll' | Select-Object -First 1 -ExpandProperty FullName
    }
}
$exe = Join-Path $here 'bin\nwd2bimc.exe'
& $csc @common '/target:exe' "/out:$exe" "/r:$autoRef" (Join-Path $here 'cli\Nwd2Bimc.cs')
if ($LASTEXITCODE -ne 0) { throw 'nwd2bimc build failed.' }
Get-ChildItem (Join-Path $here 'bin') -Filter 'Autodesk.*.dll' -Recurse -ErrorAction SilentlyContinue | Remove-Item -Force
foreach ($y in $built.Keys) {
    try { Copy-Item $exe (Join-Path $here "bin\$y") -Force -ErrorAction Stop } catch { Write-Warning "bin\$y\nwd2bimc.exe is in use - not updated" }
}
Write-Host "Built: $exe"

# --- instalare pentru utilizatorul curent, in fiecare versiune instalata
if (-not $NoInstall) {
    if (Get-Process Roamer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 }) {
        Write-Warning 'Navisworks is open - close it and run this script again to install the add-in.'
        exit 4
    }
    foreach ($i in $installed) {
        if (-not $built.ContainsKey($i.Year)) { Write-Warning "No add-in built for Navisworks $($i.Year)"; continue }
        $new = Join-Path $env:APPDATA "Autodesk\Navisworks $($i.Edition) $($i.Year)"
        $old = Join-Path $env:APPDATA "Autodesk Navisworks $($i.Edition) $($i.Year)"
        $base = if ((Test-Path $new) -or $i.Year -ge 2024) { $new } else { $old }
        $plug = Join-Path $base 'Plugins\ManageBIM.NwExporter'
        New-Item -ItemType Directory -Force $plug | Out-Null
        try { Copy-Item $built[$i.Year] $plug -Force -ErrorAction Stop; Write-Host "Installed: $plug" }
        catch { Write-Warning "Could not install into Navisworks $($i.Year) (is it running?): $_" }
    }
}
