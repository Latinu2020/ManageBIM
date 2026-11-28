<#
.SYNOPSIS
  End-to-end smoke test for the ManageBIM MCP test tools: starts a separate instance with a
  temporary profile (your open application is not touched), opens a model, runs quality checks,
  loads a 5D price catalog, prices the quantities, exports the cost report and checks the logs
  and performance counters. Fails on any MCP / tool / data error.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\test-mcp.ps1
  powershell -ExecutionPolicy Bypass -File tools\test-mcp.ps1 -Exe D:\dist\ManageBIM.exe -Model C:\models\house.ifc
#>
param(
    [string]$Exe = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release\ManageBIM.exe'),
    [string]$Model = (Join-Path (Split-Path -Parent $PSScriptRoot) 'tests\fixtures\demo_arhitectura.ifc'),
    [string]$Catalog = '',
    [int]$Port = 8768,
    [int]$TimeoutSec = 120,
    [string]$Out = (Join-Path $env:TEMP 'bim-mcp-test')
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Exe)) { throw "Executable not found: $Exe" }
if (-not (Test-Path $Model)) { throw "Model not found: $Model" }

$root = Split-Path -Parent $PSScriptRoot
$profile = Join-Path $Out 'profile'
$appdata = Join-Path $profile 'AppData\Roaming'
$local = Join-Path $profile 'AppData\Local'
$cfg = Join-Path $appdata 'ManageBIM'
New-Item -ItemType Directory -Force $cfg, $local, $Out | Out-Null

if (-not $Catalog) {
    $Catalog = Join-Path $Out 'prices.csv'
    @'
Code;Description;Unit;Unit price;Budget
IFCWALL;Perete;m3;250,50;50000
IFCSLAB;Placa;m3;180,00;40000
IFCCOLUMN;Stalp;buc;1200,00;20000
IFCBEAM;Grinda;buc;850,00;15000
IFCDOOR;Usa;buc;450,00;5000
IFCWINDOW;Fereastra;buc;650,00;8000
'@ | Set-Content -Path $Catalog -Encoding UTF8
}

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
$id = 1
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
function Text([string]$name, [hashtable]$arguments = @{}, [int]$timeout = 60) {
    $c = Call $name $arguments $timeout
    $t = $c | Where-Object type -eq 'text' | Select-Object -First 1
    if (-not $t) { throw "Tool $name returned no text" }
    return $t.text
}

$proc = $null
try {
    $old = $env:APPDATA; $oldLocal = $env:LOCALAPPDATA
    $env:APPDATA = $appdata; $env:LOCALAPPDATA = $local
    $proc = Start-Process -FilePath $Exe -PassThru
    $env:APPDATA = $old; $env:LOCALAPPDATA = $oldLocal

    $deadline = (Get-Date).AddSeconds(40)
    do {
        Start-Sleep -Milliseconds 400
        try { $init = Mcp 'initialize' @{ protocolVersion = '2025-06-18'; clientInfo = @{ name = 'mcp-smoke'; version = '1' } } 5; break } catch { }
    } while ((Get-Date) -lt $deadline)
    if (-not $init) { throw 'MCP server did not start' }
    "MCP: $($init.result.serverInfo.name) $($init.result.serverInfo.version)"

    $list = Mcp 'tools/list' @{} 20
    $names = @($list.result.tools | ForEach-Object name)
    foreach ($want in @('get_performance', 'set_performance', 'run_check', 'get_log', 'load_price_catalog', 'get_cost', 'export_cost')) {
        if ($want -notin $names) { throw "Tool $want is not advertised by tools/list" }
    }
    "Tools: $($names.Count) advertised, all 7 test tools present"

    $null = Call 'open_model' @{ path = $Model }
    $deadline = (Get-Date).AddSeconds(90)
    do {
        Start-Sleep -Milliseconds 700
        $info = Text 'get_model_info' @{} 10 | ConvertFrom-Json
    } while ($info.models.Count -eq 0 -and (Get-Date) -lt $deadline)
    if ($info.models.Count -eq 0) { throw 'The model did not load' }
    "Model: $($info.models[0].elements) elements"

    $perf = Text 'get_performance' | ConvertFrom-Json
    if ($null -eq $perf.draw -or $null -eq $perf.draw.elements) { throw 'get_performance has no draw counters' }
    "Performance: fps=$($perf.fps) elements=$($perf.draw.elements) drawn=$($perf.draw.elements_drawn) triangles=$($perf.draw.triangles_drawn) ranges=$($perf.draw.ranges)"
    $null = Call 'set_performance' @{ full_detail = $true }

    $checks = Text 'run_check' @{ mode = 'quick'; limit = 5 } 90 | ConvertFrom-Json
    if (@($checks.quick).Count -lt 5) { throw 'run_check returned no quick checks' }
    "Checks: $(@($checks.quick).Count) quick checks, $($checks.quick | Measure-Object -Property failed -Sum | Select-Object -ExpandProperty Sum) failures"

    $cat = Text 'load_price_catalog' @{ path = $Catalog } | ConvertFrom-Json
    if ($cat.items -lt 3) { throw "Price catalog loaded only $($cat.items) items" }
    "Catalog: $($cat.catalog), $($cat.items) items"
    $cost = Text 'get_cost' @{ limit = 50 } | ConvertFrom-Json
    if ($cost.matched -eq 0 -or $cost.amount_total -le 0) { throw 'get_cost: no group was priced' }
    if ($cost.variance_total -ne [Math]::Round($cost.budget_total - $cost.amount_total, 2)) { throw 'get_cost: variance does not match budget - amount' }
    "Cost: $($cost.rows.Count) rows, matched=$($cost.matched), amount=$($cost.amount_total) $($cost.currency), variance=$($cost.variance_total)"

    $dst = Join-Path $Out 'cost.xlsx'
    $ex = Text 'export_cost' @{ path = $dst; format = 'xlsx' } | ConvertFrom-Json
    if (-not (Test-Path $ex.path) -or (Get-Item $ex.path).Length -lt 1000) { throw "Cost export missing or too small: $ex.path" }
    "Export: $($ex.path) ($((Get-Item $ex.path).Length) bytes)"

    $log = Text 'get_log' @{ limit = 20 } | ConvertFrom-Json
    if ($null -eq $log.log) { throw 'get_log returned no log' }
    "Log: $(@($log.log).Count) lines, toasts=$(@($log.toasts).Count)"
    'PASS - MCP test tools, 5D cost and headless checks verified'
} finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
}
