<#
.SYNOPSIS
  Compileaza exportatorul ManageBIM pentru Revit 2023 - 2026 fara .NET SDK (doar csc din Visual Studio Build Tools):
    2023 / 2024  -> .NET Framework 4.8
    2025 / 2026  -> .NET 8 (runtime-ul .NET 8 desktop instalat)
  Referintele Revit API vin din pachetele NuGet Nice3point.Revit.Api.RevitAPI / RevitAPIUI (doar assembly-uri
  de referinta, descarcate o data in revit-addin\refs). Revit nu trebuie sa fie instalat.
  Rezultatul ajunge in installers\payload\revit\ManageBIM.RevitExporter.<an>.dll; apoi `cargo build --release`
  reface ManageBIM-Revit-AddIn.exe (care le include).
#>
param([int[]]$Versions = @(2023, 2024, 2025, 2026))
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$root = Split-Path -Parent $here
$payload = Join-Path $root 'installers\payload\revit'
$obj = Join-Path $here 'obj'
$refsRoot = Join-Path $here 'refs'
New-Item -ItemType Directory -Force $obj, $refsRoot | Out-Null

$csc = $null
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path $vswhere) { foreach ($vs in (& $vswhere -all -products * -property installationPath)) { $c = Join-Path $vs 'MSBuild\Current\Bin\Roslyn\csc.exe'; if (Test-Path $c) { $csc = $c; break } } }
if (-not $csc) { throw 'C# compiler not found (Visual Studio / Build Tools 2019+).' }
$srcs = Get-ChildItem $here -Filter *.cs | ForEach-Object { "`"$($_.FullName)`"" }
$common = @('/nologo', '/langversion:latest', '/optimize+', '/platform:x64', '/target:library', '/nowarn:1701,1702,8019,0618', "/out:`"$obj\ManageBIM.RevitExporter.dll`"")
[Net.ServicePointManager]::SecurityProtocol = 'Tls12'

function Get-RevitRefs([int]$year) {
    $dir = Join-Path $refsRoot $year
    if (-not (Get-ChildItem $dir -Recurse -Filter 'RevitAPIUI.dll' -ErrorAction SilentlyContinue)) {
        [void](New-Item -ItemType Directory -Force $dir)
        foreach ($id in 'nice3point.revit.api.revitapi', 'nice3point.revit.api.revitapiui') {
            $all = (Invoke-RestMethod "https://api.nuget.org/v3-flatcontainer/$id/index.json").versions
            $ver = $all | Where-Object { $_ -like "$year.*" -and $_ -notlike '*-*' } | Select-Object -Last 1
            if (-not $ver) { throw "No NuGet package $id for Revit $year" }
            $z = Join-Path $dir "$id.zip"
            Invoke-WebRequest "https://api.nuget.org/v3-flatcontainer/$id/$ver/$id.$ver.nupkg" -OutFile $z -UseBasicParsing
            Expand-Archive $z (Join-Path $dir $id) -Force; Remove-Item $z
        }
    }
    Get-ChildItem $dir -Recurse -Filter *.dll | Where-Object { $_.Name -in 'RevitAPI.dll', 'RevitAPIUI.dll' } | Sort-Object Name -Unique | ForEach-Object { "/r:`"$($_.FullName)`"" }
}

foreach ($y in $Versions) {
    $revit = @(Get-RevitRefs $y | Where-Object { $_ -is [string] -and $_.StartsWith('/r:') })
    if ($y -ge 2025) {
        $net8 = Get-ChildItem 'C:\Program Files\dotnet\shared\Microsoft.NETCore.App' -Directory -Filter '8.*' -ErrorAction SilentlyContinue | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
        $wd8 = Get-ChildItem 'C:\Program Files\dotnet\shared\Microsoft.WindowsDesktop.App' -Directory -Filter '8.*' -ErrorAction SilentlyContinue | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
        if (-not ($net8 -and $wd8)) { Write-Warning "Revit $y skipped: needs the .NET 8 desktop runtime."; continue }
        $byName = @{}
        foreach ($d in $net8.FullName, $wd8.FullName) {
            foreach ($f in Get-ChildItem $d -Filter *.dll) { try { [void][Reflection.AssemblyName]::GetAssemblyName($f.FullName); $byName[$f.Name.ToLower()] = $f.FullName } catch {} }
        }
        $refs = @('/nostdlib+') + @($byName.Values | ForEach-Object { "/r:`"$_`"" })
        $noconfig = @('/noconfig')
    } else {
        $fw = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
        $refs = @("$fw\System.dll", "$fw\System.Core.dll", "$fw\System.Xaml.dll", "$fw\System.Windows.Forms.dll", "$fw\System.Drawing.dll", "$fw\WPF\PresentationCore.dll", "$fw\WPF\WindowsBase.dll") | ForEach-Object { "/r:`"$_`"" }
        $noconfig = @()
    }
    $rsp = Join-Path $obj "r$y.rsp"
    ($common + $refs + $revit + $srcs) | Set-Content $rsp -Encoding ascii
    & $csc @noconfig "@$rsp"
    if ($LASTEXITCODE -ne 0) { throw "Revit $y build failed" }
    Copy-Item "$obj\ManageBIM.RevitExporter.dll" (Join-Path $payload "ManageBIM.RevitExporter.$y.dll") -Force
    Write-Host "Built Revit $y"
}
