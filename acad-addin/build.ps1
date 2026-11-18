<#
.SYNOPSIS
  Compileaza exportatorul ManageBIM pentru AutoCAD (si Plant 3D / Architecture / MEP...) fara .NET SDK:
    R25 (AutoCAD 2025+, .NET 8)          - referinte: runtime-ul .NET 8 instalat + DLL-urile AutoCAD 2025+ instalate
    R24 (AutoCAD 2021-2024, .NET 4.8)    - referinte: .NET Framework 4.8 + pachetele NuGet AutoCAD.NET 24.0.0
  Rezultatul ajunge in installers\payload\acad\ManageBIM.AcadExporter.R24|R25.dll; apoi
  `cargo build --release` reface ManageBIM-AutoCAD-AddIn.exe (care le include).
  Necesita compilatorul C# (Visual Studio / Build Tools 2019+).
#>
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$root = Split-Path -Parent $here
$payload = Join-Path $root 'installers\payload\acad'
$obj = Join-Path $here 'obj'
New-Item -ItemType Directory -Force $obj | Out-Null

$csc = $null
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path $vswhere) { foreach ($vs in (& $vswhere -all -products * -property installationPath)) { $c = Join-Path $vs 'MSBuild\Current\Bin\Roslyn\csc.exe'; if (Test-Path $c) { $csc = $c; break } } }
if (-not $csc) { throw 'C# compiler not found (Visual Studio / Build Tools 2019+).' }
$srcs = Get-ChildItem $here -Filter *.cs | ForEach-Object { "`"$($_.FullName)`"" }
$common = @('/nologo', '/langversion:latest', '/optimize+', '/platform:x64', '/target:library', '/nowarn:1701,1702,8019,0618')

# ---------------------------------------------------------------- R25: .NET 8 + AutoCAD 2025+
$acad = Get-ChildItem (Join-Path $env:ProgramFiles 'Autodesk') -Directory -Filter 'AutoCAD 20*' -ErrorAction SilentlyContinue | Where-Object { [int]($_.Name -replace '\D', '') -ge 2025 -and (Test-Path (Join-Path $_.FullName 'acdbmgd.dll')) } | Sort-Object Name | Select-Object -First 1
$net8 = Get-ChildItem 'C:\Program Files\dotnet\shared\Microsoft.NETCore.App' -Directory -Filter '8.*' -ErrorAction SilentlyContinue | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
$wd8 = Get-ChildItem 'C:\Program Files\dotnet\shared\Microsoft.WindowsDesktop.App' -Directory -Filter '8.*' -ErrorAction SilentlyContinue | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if ($acad -and $net8 -and $wd8) {
    $byName = @{}
    foreach ($d in $net8.FullName, $wd8.FullName) {
        foreach ($f in Get-ChildItem $d -Filter *.dll) { try { [void][Reflection.AssemblyName]::GetAssemblyName($f.FullName); $byName[$f.Name.ToLower()] = $f.FullName } catch {} }
    }
    $refs = @($byName.Values | ForEach-Object { "/r:`"$_`"" }) + @('acdbmgd.dll', 'accoremgd.dll', 'acmgd.dll', 'AdWindows.dll' | ForEach-Object { "/r:`"$(Join-Path $acad.FullName $_)`"" })
    $rsp = Join-Path $obj 'r25.rsp'
    (@('/nostdlib+') + $common + @("/out:`"$obj\R25.dll`"") + $refs + $srcs) | Set-Content $rsp -Encoding ascii
    & $csc /noconfig "@$rsp"
    if ($LASTEXITCODE -ne 0) { throw 'R25 build failed' }
    Copy-Item "$obj\R25.dll" (Join-Path $payload 'ManageBIM.AcadExporter.R25.dll') -Force
    Write-Host "Built R25 (.NET 8, $($acad.Name), runtime $($net8.Name))"
} else { Write-Warning 'R25 skipped: needs AutoCAD 2025+ installed and the .NET 8 desktop runtime.' }

# ---------------------------------------------------------------- R24: .NET Framework 4.8 + AutoCAD 2024 (NuGet)
$refDir = Join-Path $here 'refs\r24'
if (-not (Get-ChildItem $refDir -Recurse -Filter 'AcDbMgd.dll' -ErrorAction SilentlyContinue)) {
    New-Item -ItemType Directory -Force $refDir | Out-Null
    [Net.ServicePointManager]::SecurityProtocol = 'Tls12'
    foreach ($id in 'autocad.net.core', 'autocad.net.model', 'autocad.net') {
        $z = Join-Path $refDir "$id.zip"
        Invoke-WebRequest "https://api.nuget.org/v3-flatcontainer/$id/24.0.0/$id.24.0.0.nupkg" -OutFile $z -UseBasicParsing
        Expand-Archive $z (Join-Path $refDir $id) -Force; Remove-Item $z
    }
}
$fw = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
$refs24 = @("$fw\System.dll", "$fw\System.Core.dll", "$fw\System.Drawing.dll", "$fw\System.IO.Compression.dll", "$fw\System.Xaml.dll", "$fw\WPF\PresentationCore.dll", "$fw\WPF\PresentationFramework.dll", "$fw\WPF\WindowsBase.dll") | ForEach-Object { "/r:`"$_`"" }
$refs24 += Get-ChildItem $refDir -Recurse -Filter *.dll | Where-Object { $_.Name -in 'AcDbMgd.dll', 'AcCoreMgd.dll', 'AcMgd.dll', 'AdWindows.dll' } | Sort-Object Name -Unique | ForEach-Object { "/r:`"$($_.FullName)`"" }
$rsp = Join-Path $obj 'r24.rsp'
($common + @("/out:`"$obj\R24.dll`"") + $refs24 + $srcs) | Set-Content $rsp -Encoding ascii
& $csc "@$rsp"
if ($LASTEXITCODE -ne 0) { throw 'R24 build failed' }
Copy-Item "$obj\R24.dll" (Join-Path $payload 'ManageBIM.AcadExporter.R24.dll') -Force
Write-Host 'Built R24 (.NET Framework 4.8, AutoCAD 2021-2024)'
