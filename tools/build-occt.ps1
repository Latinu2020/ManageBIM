<#
.SYNOPSIS
  Builds OpenCascade (OCCT, LGPL-2.1 + exception) from source, only the toolkits ManageBIM needs
  for STEP / IGES / BREP (reading + XCAF + triangulation). No administrator rights.

  Result: third_party\occt\  (inc\, win64\vc14\lib\, win64\vc14\bin\ - the "Windows" layout of OCCT)
  build.rs finds it there and compiles the C++ shim (src\occt\mbocct.cpp -> mbocct.dll).

  Needs: Visual Studio 2022 Build Tools with C++ (includes CMake + Ninja). The OCCT source is downloaded
  from GitHub (Open-Cascade-SAS/OCCT, tag V8.0.1) into %LOCALAPPDATA%\ManageBIM-dev, if it is not there yet.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\build-occt.ps1
#>
param(
    [string]$Version = '8.0.1',
    [string]$Work = (Join-Path $env:LOCALAPPDATA 'ManageBIM-dev'),
    [string]$Prefix = (Join-Path (Split-Path -Parent $PSScriptRoot) 'third_party\occt'),
    [int]$Jobs = 0
)
$ErrorActionPreference = 'Stop'
function Step($m) { Write-Host "`n==> $m" -ForegroundColor Cyan }

# ------------------------------------------------------------ 1. Visual Studio (vcvars64 + CMake + Ninja)
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vsroot = $null
if (Test-Path $vswhere) { $vsroot = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath }
if (-not $vsroot) { $vsroot = Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\*", "$env:ProgramFiles\Microsoft Visual Studio\2022\*" -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName 'VC\Auxiliary\Build\vcvars64.bat') } | Select-Object -First 1 -ExpandProperty FullName }
if (-not $vsroot) { throw 'Visual Studio 2022 (Build Tools) with the C++ workload was not found.' }
$vcvars = Join-Path $vsroot 'VC\Auxiliary\Build\vcvars64.bat'
$cmakeDir = Join-Path $vsroot 'Common7\IDE\CommonExtensions\Microsoft\CMake'
$cmake = Join-Path $cmakeDir 'CMake\bin\cmake.exe'
$ninja = Join-Path $cmakeDir 'Ninja\ninja.exe'
# Ninja can also be installed per user (winget / GitHub), not only together with VS
if (-not (Test-Path $ninja)) { $ninja = (Get-Command ninja -ErrorAction SilentlyContinue).Source }
if (-not $ninja -or -not (Test-Path $ninja)) { $ninja = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages') -Recurse -Filter ninja.exe -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName }
if (-not $ninja -or -not (Test-Path $ninja)) { throw 'Ninja not found (install "Ninja" with winget or the VS C++ CMake tools component).' }
if (-not (Test-Path $cmake)) { $cmake = (Get-Command cmake -ErrorAction SilentlyContinue).Source }
if (-not $cmake -or -not (Test-Path $cmake)) { $cmake = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages') -Recurse -Filter cmake.exe -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName }
if (-not $cmake -or -not (Test-Path $cmake)) { throw 'CMake not found (install the "C++ CMake tools for Windows" component or CMake per-user).' }

# ------------------------------------------------------------ 2. OCCT source
New-Item -ItemType Directory -Force $Work | Out-Null
$src = Join-Path $Work "OCCT-$Version"
if (-not (Test-Path (Join-Path $src 'CMakeLists.txt'))) {
    $tgz = Join-Path $Work "occt-$Version.tar.gz"
    # the ManageBIM backup contains the source archive in third_party\ (rebuild without internet)
    $local = Join-Path (Split-Path -Parent $PSScriptRoot) "third_party\occt-$Version.tar.gz"
    if (-not (Test-Path $tgz) -and (Test-Path $local)) { Copy-Item $local $tgz }
    if (-not (Test-Path $tgz)) {
        Step "Downloading OCCT $Version source"
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest "https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/V$Version.tar.gz" -OutFile $tgz
    }
    Step 'Extracting'
    & tar -xzf $tgz -C $Work
    if (-not (Test-Path (Join-Path $src 'CMakeLists.txt'))) { throw "Unexpected archive layout (no $src)" }
}

# ------------------------------------------------------------ 3. configure + build
# Only what is needed: Foundation + ModelingData + ModelingAlgorithms (triangulation) + STEP/IGES data exchange
# + XCAF (assembly structure, names, colours). TKXCAF depends on TKService/TKV3d (no OpenGL / FreeType / Tcl).
$build = Join-Path $Work "build-$Version"
$extra = 'TKDESTEP TKDEIGES TKXCAF TKRWMesh TKXSBase TKDE TKCAF TKLCAF TKCDF TKVCAF TKService TKV3d'
if ($Jobs -le 0) { $Jobs = [Environment]::ProcessorCount }
$cfg = @(
    "-G", "Ninja", "-S", "`"$src`"", "-B", "`"$build`"",
    "-DCMAKE_MAKE_PROGRAM=`"$ninja`"",
    "-DCMAKE_BUILD_TYPE=Release",
    "-DINSTALL_DIR=`"$Prefix`"",
    "-DINSTALL_DIR_LAYOUT=Windows",
    "-DBUILD_LIBRARY_TYPE=Shared",
    "-DBUILD_MODULE_FoundationClasses=ON",
    "-DBUILD_MODULE_ModelingData=ON",
    "-DBUILD_MODULE_ModelingAlgorithms=ON",
    "-DBUILD_MODULE_Visualization=OFF",
    "-DBUILD_MODULE_ApplicationFramework=OFF",
    "-DBUILD_MODULE_DataExchange=OFF",
    "-DBUILD_MODULE_Draw=OFF",
    "-DBUILD_ADDITIONAL_TOOLKITS=`"$extra`"",
    "-DUSE_FREETYPE=OFF", "-DUSE_TK=OFF", "-DUSE_OPENGL=OFF", "-DUSE_GLES2=OFF", "-DUSE_D3D=OFF",
    "-DUSE_FREEIMAGE=OFF", "-DUSE_OPENVR=OFF", "-DUSE_FFMPEG=OFF", "-DUSE_TBB=OFF", "-DUSE_VTK=OFF",
    "-DUSE_RAPIDJSON=OFF", "-DUSE_DRACO=OFF", "-DUSE_TCL=OFF",
    "-DBUILD_GTEST=OFF", "-DBUILD_SAMPLES_QT=OFF", "-DBUILD_DOC_Overview=OFF", "-DBUILD_RESOURCES=OFF",
    "-DBUILD_USE_PCH=ON"
) -join ' '
Step "Configuring OCCT $Version (Ninja, Release)"
$bat = Join-Path $Work 'occt-build.cmd'
@"
@echo off
call "$vcvars" >nul || exit /b 1
"$cmake" $cfg || exit /b 2
"$cmake" --build "$build" --parallel $Jobs || exit /b 3
"$cmake" --install "$build" --prefix "$Prefix" || exit /b 4
"@ | Set-Content -Encoding ascii $bat
& cmd.exe /c "`"$bat`""
if ($LASTEXITCODE -ne 0) { throw "OCCT build failed (step $LASTEXITCODE). Log: $build" }

# licence next to the binaries (LGPL-2.1 + the OCCT exception)
Copy-Item (Join-Path $src 'LICENSE_LGPL_21.txt'), (Join-Path $src 'OCCT_LGPL_EXCEPTION.txt') $Prefix -Force
Set-Content -Encoding utf8 (Join-Path $Prefix 'VERSION.txt') "Open CASCADE Technology $Version - https://github.com/Open-Cascade-SAS/OCCT/releases/tag/V$Version"
$bin = Get-ChildItem $Prefix -Recurse -Filter 'TKernel.dll' | Select-Object -First 1
Write-Host "`nOCCT installed in $Prefix  (DLLs: $($bin.DirectoryName))" -ForegroundColor Green
