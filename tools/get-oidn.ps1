# Downloads Intel Open Image Denoise (Apache 2.0) into third_party\oidn.
# ManageBIM loads it at runtime from <exe>\oidn (copied by make-setup.ps1); without it, denoising is unavailable.
param(
    [string]$Version = '2.5.1',
    [string]$Sha256 = 'f11f91bc072a5e3a564515724cb72ab8fcfbc445c84a84c197ff9b16cc01396f'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'third_party\oidn'
$zip = Join-Path $root "third_party\oidn-$Version.x64.windows.zip"
New-Item -ItemType Directory -Force -Path (Split-Path $zip) | Out-Null
if (-not (Test-Path $zip)) {
    $url = "https://github.com/RenderKit/oidn/releases/download/v$Version/oidn-$Version.x64.windows.zip"
    Write-Host "Download $url"
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
}
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
if ($hash -ne $Sha256) { throw "SHA256 mismatch for $zip ($hash)" }
$tmp = Join-Path $env:TEMP "oidn-$Version"
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
Expand-Archive $zip -DestinationPath $tmp
$src = Get-ChildItem $tmp -Directory | Select-Object -First 1
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
New-Item -ItemType Directory -Force -Path $dest | Out-Null
# only what is needed at runtime on the CPU: OIDN + oneTBB (no CUDA / HIP / SYCL / Level Zero) + the licenses
Get-ChildItem (Join-Path $src.FullName 'bin') -Filter *.dll | Where-Object { $_.Name -match '^(OpenImageDenoise(_core|_device_cpu)?|tbb12|tbbbind.*)\.dll$' } | Copy-Item -Destination $dest
Get-ChildItem (Join-Path $src.FullName 'doc') -Include 'LICENSE.txt', 'third-party-programs.txt', 'third-party-programs-oneTBB.txt' -Recurse | Copy-Item -Destination $dest
Remove-Item $tmp -Recurse -Force
Get-ChildItem $dest | ForEach-Object { "{0,-45} {1,8:N1} MB" -f $_.Name, ($_.Length / 1MB) }
