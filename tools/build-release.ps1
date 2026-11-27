<#
.SYNOPSIS
  Builds ManageBIM (release), digitally signs the exe files and makes the ManageBIM-Windows-x64 package.

.DESCRIPTION
  Steps:
    1. (optional) sign the add-in DLLs in installers\payload (they are built into the installers)
    2. cargo build --release for ManageBIM, bimc-convert and the add-in installers (if their sources exist)
    3. sign every .exe with signtool (SHA-256 + timestamp) and verify the signature
    4. copy everything into dist\ManageBIM-Windows-x64\ManageBIM and make a .zip

  The certificate is chosen from environment variables (the first one found wins):

    A) Certificate from the Windows Certificate Store (USB / EV token included):
         $env:MANAGEBIM_SIGN_THUMBPRINT = "AB12...EF"      (SHA1 thumbprint of the certificate)

    B) .pfx file:
         $env:MANAGEBIM_PFX          = "C:\certs\managebim.pfx"
         $env:MANAGEBIM_PFX_PASSWORD = "..."                (never write it in the script / git)

    C) Azure Trusted Signing (Artifact Signing):
         $env:MANAGEBIM_TS_DLIB     = "C:\tools\TrustedSigning\bin\x64\Azure.CodeSigning.Dlib.dll"
         $env:MANAGEBIM_TS_METADATA = "C:\tools\TrustedSigning\metadata.json"
       (Azure sign-in: `az login` or the variables AZURE_CLIENT_ID / AZURE_TENANT_ID / AZURE_CLIENT_SECRET)

  NOTE: Smart App Control accepts only signatures from a trusted authority.
  A self-signed certificate does NOT unblock the application on other computers.

.PARAMETER RequireSign
  Stops the build with an error if no certificate is configured (recommended for public releases).

.PARAMETER SkipBuild
  Does not rebuild; only signs and packages what is already in target\release.

.PARAMETER SignPayload
  Also signs the Revit / AutoCAD add-in DLLs in installers\payload before the build.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\build-release.ps1 -RequireSign -SignPayload
#>
[CmdletBinding()]
param(
    [switch]$RequireSign,
    [switch]$SkipBuild,
    [switch]$SignPayload
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot          # the ManageBIM-source folder
$release = Join-Path $root 'target\release'
$dist = Join-Path $root 'dist\ManageBIM-Windows-x64'
$pkg = Join-Path $dist 'ManageBIM'

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }

# ------------------------------------------------------------------ signtool
function Find-SignTool {
    $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    if (Test-Path $kits) {
        $found = Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\' } |
            Sort-Object { $_.Directory.Parent.Name } -Descending |
            Select-Object -First 1
        if ($found) { return $found.FullName }
    }
    return $null
}

# the signtool arguments depending on the certificate source; $null = nothing is configured
function Get-SignArgs {
    if ($env:MANAGEBIM_SIGN_THUMBPRINT) {
        return @{ Mode = 'Certificate store'; Args = @('/sha1', $env:MANAGEBIM_SIGN_THUMBPRINT, '/fd', 'sha256', '/tr', 'http://timestamp.digicert.com', '/td', 'sha256') }
    }
    if ($env:MANAGEBIM_PFX) {
        if (-not (Test-Path $env:MANAGEBIM_PFX)) { throw "MANAGEBIM_PFX points to a missing file: $env:MANAGEBIM_PFX" }
        $a = @('/f', $env:MANAGEBIM_PFX)
        if ($env:MANAGEBIM_PFX_PASSWORD) { $a += @('/p', $env:MANAGEBIM_PFX_PASSWORD) }
        $a += @('/fd', 'sha256', '/tr', 'http://timestamp.digicert.com', '/td', 'sha256')
        return @{ Mode = 'PFX file'; Args = $a }
    }
    if ($env:MANAGEBIM_TS_DLIB -and $env:MANAGEBIM_TS_METADATA) {
        foreach ($p in @($env:MANAGEBIM_TS_DLIB, $env:MANAGEBIM_TS_METADATA)) {
            if (-not (Test-Path $p)) { throw "Trusted Signing file not found: $p" }
        }
        return @{ Mode = 'Azure Trusted Signing'; Args = @('/fd', 'sha256', '/tr', 'http://timestamp.acs.microsoft.com', '/td', 'sha256', '/dlib', $env:MANAGEBIM_TS_DLIB, '/dmdf', $env:MANAGEBIM_TS_METADATA) }
    }
    return $null
}

function Invoke-Sign([string[]]$files) {
    if (-not $script:sign) { return }
    foreach ($f in $files) {
        Write-Host "   signing $(Split-Path -Leaf $f)"
        & $script:signtool sign /v $script:sign.Args $f | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "signtool sign failed for $f (exit $LASTEXITCODE)" }
        # /pa = the standard Authenticode policy (the one Windows checks too)
        & $script:signtool verify /pa /q $f
        if ($LASTEXITCODE -ne 0) { throw "Signature verification failed for $f" }
    }
}

# ------------------------------------------------------------------ 0. signing setup
Step 'Code signing setup'
$script:sign = Get-SignArgs
$script:signtool = $null
if ($script:sign) {
    $script:signtool = Find-SignTool
    if (-not $script:signtool) {
        throw 'signtool.exe not found. Install the Windows SDK (component "Windows SDK Signing Tools for Desktop Apps").'
    }
    Write-Host "   mode:     $($script:sign.Mode)"
    Write-Host "   signtool: $($script:signtool)"
} elseif ($RequireSign) {
    throw 'No signing certificate configured (MANAGEBIM_SIGN_THUMBPRINT / MANAGEBIM_PFX / MANAGEBIM_TS_*). See the header of this script.'
} else {
    Write-Warning 'No signing certificate configured - building UNSIGNED. Smart App Control will block these exe files.'
}

# ------------------------------------------------------------------ 1. payload add-in
if ($SignPayload) {
    Step 'Signing add-in payload DLLs (installers\payload)'
    $dlls = Get-ChildItem (Join-Path $root 'installers\payload') -Recurse -Filter *.dll | ForEach-Object { $_.FullName }
    Invoke-Sign $dlls
}

# ------------------------------------------------------------------ 2. build
$bins = @(
    @{ Name = 'ManageBIM';               Src = 'src\main.rs';              Dest = '' },
    @{ Name = 'bimc-convert';            Src = 'src\bin\bimc_convert.rs';  Dest = '' },
    @{ Name = 'ManageBIM-Revit-AddIn';   Src = 'src\bin\addin\revit.rs';   Dest = 'AddIns' },
    @{ Name = 'ManageBIM-AutoCAD-AddIn'; Src = 'src\bin\addin\acad.rs';    Dest = 'AddIns' }
)
$present = @($bins | Where-Object { Test-Path (Join-Path $root $_.Src) })
$absent = @($bins | Where-Object { -not (Test-Path (Join-Path $root $_.Src)) })
foreach ($b in $absent) { Write-Warning "Source missing, skipping $($b.Name): $($b.Src)" }
if ($present.Count -eq 0) { throw 'No binary sources found.' }

if (-not $SkipBuild) {
    Step 'cargo build --release'
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'cargo not found. Install Rust from https://rustup.rs (MSVC toolchain).' }
    $cargoArgs = @('build', '--release')
    foreach ($b in $present) { $cargoArgs += @('--bin', $b.Name) }
    Push-Location $root
    try {
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
    } finally { Pop-Location }
}

# ------------------------------------------------------------------ 3. sign the exes
Step 'Signing executables'
$exes = @()
foreach ($b in $present) {
    $exe = Join-Path $release "$($b.Name).exe"
    if (-not (Test-Path $exe)) { throw "Missing build output: $exe" }
    $exes += $exe
}
Invoke-Sign $exes
if (-not $script:sign) { Write-Host '   (skipped - unsigned build)' }

# ------------------------------------------------------------------ 3b. Navisworks add-in (if Navisworks + a C# compiler are installed)
$navis = Join-Path $root 'navis-addin'
$navisBin = $null
if (Test-Path (Join-Path $navis 'build.ps1')) {
    Step 'Navisworks add-in'
    try {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $navis 'build.ps1') -NoInstall
        $navisBin = Get-ChildItem (Join-Path $navis 'bin') -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
        if ($navisBin) { Invoke-Sign @((Join-Path $navisBin.FullName 'ManageBIM.NwExporter.dll'), (Join-Path $navisBin.FullName 'nwd2bimc.exe')) }
    } catch {
        Write-Warning "Navisworks add-in skipped: $($_.Exception.Message)"
    }
}

# ------------------------------------------------------------------ 4. package
Step "Packaging to $dist"
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $pkg 'AddIns') | Out-Null
foreach ($b in $present) {
    $dest = if ($b.Dest) { Join-Path $pkg $b.Dest } else { $pkg }
    Copy-Item (Join-Path $release "$($b.Name).exe") $dest
}
if ($navisBin) {
    $nd = Join-Path $pkg 'AddIns\Navisworks'
    New-Item -ItemType Directory -Force $nd | Out-Null
    Copy-Item (Join-Path $navisBin.FullName 'ManageBIM.NwExporter.dll'), (Join-Path $navisBin.FullName 'nwd2bimc.exe') $nd
    Copy-Item (Join-Path $navis 'install.ps1'), (Join-Path $navis 'install.cmd'), (Join-Path $navis 'README.md') $nd -ErrorAction SilentlyContinue
}
Copy-Item (Join-Path $root 'BIMC_FORMAT.md') $pkg
# OpenCascade (native STEP / IGES / BREP): occt\ next to ManageBIM.exe. We sign the DLLs we built
# (mbocct + TK*); the MSVC runtime is already signed by Microsoft.
$occt = Join-Path $release 'occt'
if (Test-Path (Join-Path $occt 'mbocct.dll')) {
    Copy-Item $occt (Join-Path $pkg 'occt') -Recurse
    Invoke-Sign @(Get-ChildItem (Join-Path $pkg 'occt') -Filter '*.dll' | Where-Object { $_.Name -like 'TK*' -or $_.Name -eq 'mbocct.dll' } | ForEach-Object FullName)
} else { Write-Warning 'OpenCascade runtime missing (run tools\build-occt.ps1 before the release build) - STEP / IGES / BREP / FCStd and DWG solids will not open.' }
$samples = Join-Path $root 'samples'
if (Test-Path $samples) { Copy-Item $samples (Join-Path $pkg 'examples') -Recurse }

$zip = "$dist.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $pkg -DestinationPath $zip

Step 'Done'
Get-ChildItem $pkg -Recurse -Filter *.exe | ForEach-Object {
    $s = Get-AuthenticodeSignature $_.FullName
    $who = if ($s.SignerCertificate) { $s.SignerCertificate.Subject } else { '-' }
    '{0,-30} {1,-10} {2}' -f $_.Name, $s.Status, $who
}
Write-Host "`nPackage: $pkg`nZip:     $zip"
