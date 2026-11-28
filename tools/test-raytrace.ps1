<#
.SYNOPSIS
  Headless-ish smoke test for the interactive Ray Trace view, through ManageBIM's own MCP server.
  Starts a separate instance with a temporary profile (your open application is not touched), opens a
  model, turns Ray Trace on, waits until the GPU accumulation converges, captures the LIVE view and
  fails on a GPU error or a timeout.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\test-raytrace.ps1
  powershell -ExecutionPolicy Bypass -File tools\test-raytrace.ps1 -Exe D:\dist\ManageBIM.exe -Model C:\models\house.ifc
#>
param(
    [string]$Exe = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release\ManageBIM.exe'),
    [string]$Model = (Join-Path $env:USERPROFILE 'Downloads\Snowdon Towers Sample HVAC.ifc'),
    [int]$Port = 8767,
    [int]$TimeoutSec = 240,
    [string]$Out = (Join-Path $env:TEMP 'bim-raytrace-test')
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Exe)) { throw "Executable not found: $Exe" }
if (-not (Test-Path $Model)) { throw "Model not found: $Model" }

$root = Split-Path -Parent $PSScriptRoot
$profile = Join-Path $Out 'profile'
$appdata = Join-Path $profile 'AppData\Roaming'
$local = Join-Path $profile 'AppData\Local'
$cfg = Join-Path $appdata 'ManageBIM'
New-Item -ItemType Directory -Force $cfg, (Join-Path $local 'ManageBIM') | Out-Null

# copy the real settings, then enable MCP in the temporary copy only
$src = Join-Path $env:APPDATA 'ManageBIM\settings.json'
if (Test-Path $src) {
    $settings = Get-Content $src -Raw | ConvertFrom-Json
} else {
    $settings = [PSCustomObject]@{ recent = @() }
}
if (-not $settings.PSObject.Properties['mcp']) {
    $settings | Add-Member -NotePropertyName mcp -NotePropertyValue ([PSCustomObject]@{ enabled = $false; port = 8765; token = ''; allow_edit = $true })
}
$settings.mcp.enabled = $true
$settings.mcp.port = $Port
$settings.mcp.token = [guid]::NewGuid().ToString('N')
$settings.mcp.allow_edit = $true
[IO.File]::WriteAllText((Join-Path $cfg 'settings.json'), ($settings | ConvertTo-Json -Depth 12), (New-Object Text.UTF8Encoding $false))

$token = $settings.mcp.token
$url = "http://127.0.0.1:$Port/mcp"
$headers = @{ 'Content-Type' = 'application/json'; 'Accept' = 'application/json'; 'Authorization' = "Bearer $token" }
function Mcp([string]$method, [hashtable]$params = @{}, [int]$timeout = 60) {
    $body = @{ jsonrpc = '2.0'; id = $id++; method = $method }
    if ($params.Count) { $body.params = $params }
    $json = $body | ConvertTo-Json -Compress -Depth 10
    return Invoke-RestMethod -Uri $url -Method Post -Headers $headers -Body $json -TimeoutSec $timeout
}
function Call([string]$name, [hashtable]$arguments = @{}, [int]$timeout = 60) {
    $r = Mcp 'tools/call' @{ name = $name; arguments = $arguments } $timeout
    if ($r.error) { throw "MCP error: $($r.error.message)" }
    if ($r.result.isError) { throw "Tool $name failed: $($r.result.content[0].text)" }
    return $r.result.content
}
$id = 1
$proc = $null
try {
    $old = $env:APPDATA; $oldLocal = $env:LOCALAPPDATA
    $env:APPDATA = $appdata; $env:LOCALAPPDATA = $local
    $proc = Start-Process -FilePath $Exe -PassThru
    $env:APPDATA = $old; $env:LOCALAPPDATA = $oldLocal

    # wait for the server
    $deadline = (Get-Date).AddSeconds(40)
    do {
        Start-Sleep -Milliseconds 400
        try { $init = Mcp 'initialize' @{ protocolVersion = '2025-06-18'; clientInfo = @{ name = 'raytrace-smoke'; version = '1' } } 5; break } catch { }
    } while ((Get-Date) -lt $deadline)
    if (-not $init) { throw 'MCP server did not start' }
    "MCP: $($init.result.serverInfo.name) $($init.result.serverInfo.version)"

    # load a model
    $null = Call 'open_model' @{ path = $Model }
    $deadline = (Get-Date).AddSeconds(90)
    do {
        Start-Sleep -Milliseconds 700
        $info = (Call 'get_model_info' @{} 10 | Where-Object type -eq 'text' | Select-Object -First 1).text | ConvertFrom-Json
    } while ($info.models.Count -eq 0 -and (Get-Date) -lt $deadline)
    if ($info.models.Count -eq 0) { throw 'The model did not load' }
    "Model: $($info.models[0].elements) elements, $($info.models[0].triangles) triangles"

    $state = (Call 'render_settings' @{ ray_trace = $true } | Where-Object type -eq 'text' | Select-Object -First 1).text | ConvertFrom-Json
    if (-not $state.ray_trace) { throw 'Ray Trace did not turn on' }

    # Watch the first seconds: the resolution must not flip between half and full, and the sample
    # counter must not jump back to 0 (both were the "black window / flicker" regression).
    $size0 = ''; $prev = 0
    for ($k = 0; $k -lt 10; $k++) {
        Start-Sleep -Milliseconds 600
        $q = (Call 'get_render_status' @{} 15 | Where-Object type -eq 'text' | Select-Object -First 1).text | ConvertFrom-Json
        if ($q.gpu_error) { throw "GPU path tracing error: $($q.gpu_error)" }
        if (-not $q.ray_trace) { throw 'Ray Trace turned itself off while sampling' }
        $size = "$($q.width)x$($q.height)"
        if ($size0 -eq '') { $size0 = $size }
        if ($q.samples -ge 1 -and $size -ne $size0) { throw "resolution changed from $size0 to $size (flicker)" }
        if ($q.samples -ge 1 -and $q.samples -lt $prev) { throw "sample counter reset from $prev to $($q.samples)" }
        if ($q.samples -ge 1) { $prev = $q.samples }
    }
    "Stability: resolution $size0 constant, samples monotonic up to $prev"

    # wait until the live accumulation converged (or the denoiser finished)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    do {
        Start-Sleep -Milliseconds 500
        $s = (Call 'get_render_status' @{} 15 | Where-Object type -eq 'text' | Select-Object -First 1).text | ConvertFrom-Json
        if ($s.gpu_error) { throw "GPU path tracing error: $($s.gpu_error)" }
        if ($s.samples -ge $s.max_samples -or $s.denoised) { break }
    } while ((Get-Date) -lt $deadline)
    "Ray Trace: $($s.samples)/$($s.max_samples) samples, $($s.width)x$($s.height), denoised=$($s.denoised)"
    if ($s.samples -lt $s.max_samples -and -not $s.denoised) { throw "Ray Trace did not converge in $TimeoutSec s (stuck at $($s.samples)/$($s.max_samples))" }

    # capture the LIVE view (not a new offscreen render)
    $img = (Call 'capture_view' @{ live = $true; width = 1024; height = 576 } 120 | Where-Object type -eq 'image' | Select-Object -First 1)
    if (-not $img -or -not $img.data) { throw 'Live capture returned no image' }
    $dst = Join-Path $Out 'raytrace-live.jpg'
    [IO.File]::WriteAllBytes($dst, [Convert]::FromBase64String($img.data))
    if ((Get-Item $dst).Length -lt 1000) { throw "Live capture is too small: $dst" }
    "Image: $dst ($((Get-Item $dst).Length) bytes)"
    'PASS - interactive Ray Trace converged without errors'
} finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
}
