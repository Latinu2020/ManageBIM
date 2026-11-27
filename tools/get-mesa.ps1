# Downloads Mesa's software OpenGL (llvmpipe, MIT) from the mesa-dist-win builds into third_party\mesa.
# ManageBIM uses it only when the graphics card / driver has no OpenGL 3.3 (virtual machines, old Remote
# Desktop, very old drivers), or when Settings › Software rendering is on: it is loaded from <exe>\mesa
# (copied by make-setup.ps1). Without it, those PCs get a clear message instead of a 3D view.
param(
    [string]$Version = '26.3.1',
    [string]$Sha256 = '64e84cf13d67dd354640bf443ed0445e6a83afd06a5729f0750b3744d633b3f1'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'third_party\mesa'
$arc = Join-Path $root "third_party\mesa3d-$Version-release-msvc.7z"
New-Item -ItemType Directory -Force -Path (Split-Path $arc) | Out-Null
if (-not (Test-Path $arc)) {
    $url = "https://github.com/pal1000/mesa-dist-win/releases/download/$Version/mesa3d-$Version-release-msvc.7z"
    Write-Host "Download $url"
    Invoke-WebRequest -Uri $url -OutFile $arc -UseBasicParsing
}
$hash = (Get-FileHash $arc -Algorithm SHA256).Hash.ToLower()
if ($hash -ne $Sha256) { throw "SHA256 mismatch for $arc ($hash)" }
$tmp = Join-Path $env:TEMP "mesa-$Version"
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
# the tar of Windows 10 / 11 (libarchive) reads .7z; only the 64-bit OpenGL driver and the readme are needed
& "$env:SystemRoot\System32\tar.exe" -xf $arc -C $tmp 'x64/opengl32.dll' 'x64/libgallium_wgl.dll' 'readme.txt'
if ($LASTEXITCODE -ne 0) { throw 'Could not extract the Mesa archive (tar).' }
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item (Join-Path $tmp 'x64\opengl32.dll'), (Join-Path $tmp 'x64\libgallium_wgl.dll') $dest
Copy-Item (Join-Path $tmp 'readme.txt') (Join-Path $dest 'mesa-dist-win-readme.txt')
@"
Mesa 3D $Version - software OpenGL (llvmpipe), Windows build by mesa-dist-win (https://github.com/pal1000/mesa-dist-win).
Mesa is licensed under the MIT license (https://docs.mesa3d.org/license.html); the mesa-dist-win build scripts
are MIT licensed, Copyright (c) 2017-2020 pal1000. LLVM (used by llvmpipe) is under Apache-2.0 with LLVM exceptions.
"@ | Set-Content (Join-Path $dest 'LICENSE.txt') -Encoding utf8
Remove-Item $tmp -Recurse -Force
Get-ChildItem $dest | ForEach-Object { "{0,-30} {1,8:N1} MB" -f $_.Name, ($_.Length / 1MB) }
