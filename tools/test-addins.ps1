<#
.SYNOPSIS
  Automated smoke test of the ManageBIM add-ins in the Autodesk programs installed on this PC.

.DESCRIPTION
  Runs without user interaction and leaves the user's settings and Inbox alone (everything goes to
  target\addin-test). For every program it finds:
   - AutoCAD / Plant 3D (accoreconsole.exe, every installed year): loads the add-in built for that year
     (R24 = 2021-2024, R25 = 2025+), draws a box, runs BIMCSEND (model -> .bimc), MBLEVELS
     (levels -> lines, UCS, XData) and MBREFERENCE (a ManageBIM reference from tests\fixtures\addins),
     saves the drawing. Checks the .bimc and the console messages.
   - Navisworks Manage / Simulate (Automation API, every installed year): exports a sample IFC
     to .bimc with the ManageBIM add-in (nwd2bimc.exe) and checks the file; then runs ManageBIM Levels
     (selection sets + plan viewpoints), Walk path (viewpoint animation) and Issues (viewpoints with
     section box and markups) on it.
   - Revit (every installed year with the add-in): starts Revit with a sample model and the add-in's
     self-test (MANAGEBIM_AUTOTEST), which exports the 3D view like "Send to ManageBIM" and closes Revit.
  Exit code: 0 all passed, 1 a test failed, 2 nothing to test.

  Build the add-ins first (revit-addin\build.ps1, acad-addin\build.ps1, navis-addin\build.ps1).
  Navisworks loads the add-in from its Plugins folder: run navis-addin\build.ps1 without -NoInstall.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\test-addins.ps1
#>
param([int]$TimeoutSec = 300, [switch]$SkipRevit)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$work = Join-Path $root 'target\addin-test'
$pf = $env:ProgramFiles
$results = @()

function Add-Result($name, $ok, $detail) {
    $script:results += [pscustomobject]@{ Test = $name; Result = $(if ($ok) { 'PASS' } else { 'FAIL' }); Detail = $detail }
    $c = if ($ok) { 'Green' } else { 'Red' }
    Write-Host ("{0,-34} {1}  {2}" -f $name, $(if ($ok) { 'PASS' } else { 'FAIL' }), $detail) -ForegroundColor $c
}

# a .bimc starts with the magic "BIMC" and holds at least the header
function Test-Bimc($path) {
    if (-not (Test-Path $path)) { return 'missing' }
    $b = [IO.File]::ReadAllBytes($path)
    if ($b.Length -lt 16) { return "too small ($($b.Length) bytes)" }
    if ([Text.Encoding]::ASCII.GetString($b, 0, 4) -ne 'BIMC') { return 'not a .bimc file' }
    return $null
}

# ------------------------------------------------------------------ AutoCAD / Plant 3D
$acads = Get-ChildItem (Join-Path $pf 'Autodesk') -Directory -Filter 'AutoCAD*' -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^AutoCAD (\d{4})$' -and (Test-Path (Join-Path $_.FullName 'accoreconsole.exe')) }
foreach ($a in $acads) {
    $year = [int]($a.Name -replace '\D', '')
    $series = if ($year -ge 2025) { 'R25' } else { 'R24' }
    $dll = Join-Path $root "installers\payload\acad\ManageBIM.AcadExporter.$series.dll"
    $name = "AutoCAD $year ($series)"
    if (-not (Test-Path $dll)) { Add-Result $name $false "add-in not built: $dll"; continue }
    $dir = Join-Path $work "acad$year"
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force (Join-Path $dir 'inbox') | Out-Null
    "MBLEVELS1`nL`tGround Floor`t0`nL`tLevel 1`t3.2`nL`tLevel 2`t6.4`n" | Set-Content (Join-Path $dir 'levels.txt') -Encoding ascii -NoNewline
    $dwg = Join-Path $dir 'out.dwg'
    $ref = Join-Path $root 'tests\fixtures\addins\wall_opening.mbref'
    @(
        'SECURELOAD 0', 'FILEDIA 0', "NETLOAD `"$dll`"",
        '_BOX 0,0,0 2,3,4',
        'BIMCSEND',
        'MBLEVELS',
        'MBREFERENCE', "`"$ref`"",
        "_SAVEAS 2018 `"$dwg`"",
        '_QUIT _Y', ''
    ) -join "`r`n" | Set-Content (Join-Path $dir 'test.scr') -Encoding ascii
    $env:MANAGEBIM_INBOX = Join-Path $dir 'inbox'
    $env:MANAGEBIM_NO_LAUNCH = '1'
    $env:MB_LEVELS_FILE = Join-Path $dir 'levels.txt'
    $log = Join-Path $dir 'console.log'
    $p = Start-Process (Join-Path $a.FullName 'accoreconsole.exe') -ArgumentList '/s', "`"$(Join-Path $dir 'test.scr')`"" -RedirectStandardOutput $log -PassThru -WindowStyle Hidden
    if (-not $p.WaitForExit($TimeoutSec * 1000)) { $p.Kill(); Add-Result $name $false "timed out after $TimeoutSec s (see $log)"; continue }
    $out = (Get-Content $log -Raw) -replace "`0", ''
    $bimc = Get-ChildItem (Join-Path $dir 'inbox') -Filter *.bimc -ErrorAction SilentlyContinue | Select-Object -First 1
    $err = if ($bimc) { Test-Bimc $bimc.FullName } else { 'no .bimc in the Inbox' }
    Add-Result "$name BIMCSEND" (-not $err -and $out -match 'ManageBIM: 1 objects') $(if ($err) { $err } else { "$($bimc.Length) bytes" })
    Add-Result "$name MBLEVELS" ($out -match '3 level lines' -and $out -match 'level written on 1 objects') $(if ($out -match '3 level lines') { '3 levels, UCS, XData' } else { "see $log" })
    Add-Result "$name MBREFERENCE" ($out -match 'loaded - (\d+) meshes') $(if ($out -match 'loaded - (\d+) meshes') { "$($matches[1]) reference meshes on a locked layer" } else { "see $log" })
    Add-Result "$name no errors" (-not ($out -match 'stopped because of an error|Unhandled|Exception')) $(if (Test-Path $dwg) { 'drawing saved' } else { 'drawing not saved' })
}
Remove-Item Env:\MANAGEBIM_INBOX, Env:\MANAGEBIM_NO_LAUNCH, Env:\MB_LEVELS_FILE -ErrorAction SilentlyContinue

# ------------------------------------------------------------------ Navisworks
$navis = Get-ChildItem (Join-Path $pf 'Autodesk') -Directory -Filter 'Navisworks*' -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^Navisworks (Manage|Simulate) (\d{4})$' -and (Test-Path (Join-Path $_.FullName 'Autodesk.Navisworks.Automation.dll')) }
$cli = Join-Path $root 'navis-addin\bin\nwd2bimc.exe'
$ifc = Join-Path $root 'tests\fixtures\demo_arhitectura.ifc'
foreach ($n in $navis) {
    $year = [int]($n.Name -replace '\D', '')
    $name = "Navisworks $year"
    if (-not (Test-Path $cli)) { Add-Result $name $false "nwd2bimc.exe not built (navis-addin\build.ps1)"; continue }
    $dir = Join-Path $work "navis$year"
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $dir | Out-Null
    $out = Join-Path $dir 'demo.bimc'
    $p = Start-Process $cli -ArgumentList '--year', $year, '-f', "`"$ifc`"", "`"$out`"" -RedirectStandardOutput (Join-Path $dir 'cli.log') -RedirectStandardError (Join-Path $dir 'cli.err') -PassThru -WindowStyle Hidden
    $null = $p.Handle  # PowerShell 5.1: without this ExitCode stays empty
    if (-not $p.WaitForExit($TimeoutSec * 1000)) { $p.Kill(); Add-Result "$name export" $false "timed out after $TimeoutSec s"; continue }
    $err = Test-Bimc $out
    Add-Result "$name export" ($p.ExitCode -eq 0 -and -not $err) $(if ($err) { "$err (exit $($p.ExitCode), see $dir\cli.log)" } else { "$((Get-Item $out).Length) bytes" })

    # Levels and Walk path through the Automation API (the plugins' "quiet" mode, input files given with file=)
    "MBLEVELS1`nL`tGround Floor`t0`nL`tLevel 1`t3.2`n" | Set-Content (Join-Path $dir 'levels.txt') -Encoding ascii -NoNewline
    "MBWALK1`nNAME`tTest walk`nSPEED`t1.4`nEYE`t1.7`nP`t0 0 0`nP`t5 0 0`nP`t5 5 0`n" | Set-Content (Join-Path $dir 'walk.txt') -Encoding ascii -NoNewline
    ("VIEW`tEntrance`tOpen`t1b4e28ba-2fa1-11d2-883f-0016d3cca427`tHigh`tMEP`nCAM`t-8 -8 4`t0.7 0.7 -0.2`t0 0 1`t50`t0`nTXT`tCheck the door`nEND`n" +
     "VIEW`tPlan`tActive`t1b4e28ba-2fa1-11d2-883f-0016d3cca428`tNormal`t`nCAM`t5 5 30`t0 0 -1`t0 1 0`t0`t20`nBOX`t0 0 0`t10 10 3`nRED`t230 40 40`t3`t-0.1 -0.1 0.1 0.1`nEND`n") | Set-Content (Join-Path $dir 'issues.txt') -Encoding ascii -NoNewline
    $auto = Join-Path $dir 'auto.ps1'
    @'
param($NwDir, $Ifc, $Levels, $Walk, $Issues)
$ErrorActionPreference = 'Stop'
Add-Type -Path (Join-Path $NwDir 'Autodesk.Navisworks.Automation.dll')
$app = New-Object Autodesk.Navisworks.Api.Automation.NavisworksApplication
try {
    $app.DisableProgress()
    $app.OpenFile($Ifc)
    "LEVELS=" + $app.ExecuteAddInPlugin('ManageBIMLevels.MBIM', 'quiet', "file=$Levels")
    "WALK=" + $app.ExecuteAddInPlugin('ManageBIMWalk.MBIM', 'quiet', "file=$Walk")
    "ISSUES=" + $app.ExecuteAddInPlugin('ManageBIMIssues.MBIM', 'quiet', "file=$Issues")
} finally { $app.Dispose() }
'@ | Set-Content $auto -Encoding ascii
    $log = Join-Path $dir 'auto.log'
    $p = Start-Process powershell -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$auto`"", '-NwDir', "`"$($n.FullName)`"", '-Ifc', "`"$ifc`"", '-Levels', "`"$(Join-Path $dir 'levels.txt')`"", '-Walk', "`"$(Join-Path $dir 'walk.txt')`"", '-Issues', "`"$(Join-Path $dir 'issues.txt')`"" -RedirectStandardOutput $log -RedirectStandardError (Join-Path $dir 'auto.err') -PassThru -WindowStyle Hidden
    $null = $p.Handle
    if (-not $p.WaitForExit($TimeoutSec * 1000)) { $p.Kill(); Add-Result "$name Levels / Walk" $false "timed out after $TimeoutSec s"; continue }
    $txt = Get-Content $log -Raw -ErrorAction SilentlyContinue
    Add-Result "$name Levels" ($txt -match 'LEVELS=0') $(if ($txt -match 'LEVELS=(\d+)') { "plugin returned $($matches[1]) (0 = sets and plan viewpoints made)" } else { "see $log" })
    # Walk: 0 = viewpoint animation, 4 = folder of viewpoints (both are a success)
    Add-Result "$name Walk path" ($txt -match 'WALK=(0|4)') $(if ($txt -match 'WALK=(\d+)') { "plugin returned $($matches[1])" } else { "see $log" })
    Add-Result "$name Issues" ($txt -match 'ISSUES=0') $(if ($txt -match 'ISSUES=(\d+)') { "plugin returned $($matches[1]) (0 = both viewpoints added, with section box and markup)" } else { "see $log" })
}

# ------------------------------------------------------------------ Revit (self-test)
# Revit has no headless mode: the add-in has a self-test that runs only with MANAGEBIM_AUTOTEST set - Revit
# opens a sample model, the add-in exports its 3D view like "Send to ManageBIM", writes revit.result and
# closes Revit. Only the Revit process started here is ever stopped.
$revits = Get-ChildItem (Join-Path $pf 'Autodesk') -Directory -Filter 'Revit 20*' -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^Revit (\d{4})$' -and (Test-Path (Join-Path $_.FullName 'Revit.exe')) }
if ($SkipRevit) { $revits = @() }
foreach ($r in $revits) {
    $year = [int]($r.Name -replace '\D', '')
    $name = "Revit $year"
    if (-not (Test-Path "$env:APPDATA\Autodesk\Revit\Addins\$year\ManageBIM.addin")) { Add-Result $name $false 'ManageBIM add-in not installed for this version'; continue }
    if (Get-Process Revit -ErrorAction SilentlyContinue) { Add-Result $name $false 'Revit is already running - close it to run this test'; continue }
    $model = @(
        (Join-Path $r.FullName 'Samples\Snowdon Towers Sample Structural.rvt'),
        "$env:USERPROFILE\Documents\Snowdon Towers Sample Structural.rvt",
        (Join-Path $r.FullName 'Samples\Snowdon Towers Sample Architectural.rvt')
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $model) { Add-Result $name $false 'no sample model found'; continue }
    $dir = Join-Path $work "revit$year"
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $dir | Out-Null
    $env:MANAGEBIM_AUTOTEST = $dir
    $p = Start-Process (Join-Path $r.FullName 'Revit.exe') -ArgumentList "`"$model`"" -PassThru
    Remove-Item Env:\MANAGEBIM_AUTOTEST -ErrorAction SilentlyContinue
    $resFile = Join-Path $dir 'revit.result'
    $limit = [Math]::Max($TimeoutSec, 600)
    $t0 = Get-Date
    while (-not (Test-Path $resFile) -and -not $p.HasExited -and ((Get-Date) - $t0).TotalSeconds -lt $limit) { Start-Sleep -Seconds 3 }
    $res = if (Test-Path $resFile) { (Get-Content $resFile -Raw).Trim() } else { $null }
    $closedByItself = $p.HasExited
    # Revit closes itself after the test; if it is still there after a minute, stop it (only this process)
    if (-not $p.WaitForExit(60000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    $err = Test-Bimc (Join-Path $dir 'revit.bimc')
    if (-not $res) {
        # the newest Revit journal says what Revit was waiting for
        $jdir = "$env:LOCALAPPDATA\Autodesk\Revit\Autodesk Revit $year\Journals"
        $j = Get-ChildItem $jdir -Filter 'journal*.txt' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        $jt = if ($j) { Get-Content $j.FullName -Raw -ErrorAction SilentlyContinue } else { '' }
        $res = if ($jt -match 'TaskDialog_Security_Unsigned_File_Loading') {
            'Revit waits for you to allow the unsigned ManageBIM add-in: open Revit once, choose "Always Load", then run the test again'
        } elseif ($closedByItself) {
            'Revit closed without a result (see %LOCALAPPDATA%\ManageBIM\addin.log)'
        } else {
            "no result after $limit s - Revit was waiting (a dialog or a slow start); stopped"
        }
    }
    Add-Result "$name Send (3D view)" ($res -like 'OK*' -and -not $err) $(if ($err -and $res -like 'OK*') { $err } else { $res })
}

Write-Host ''
if ($results.Count -eq 0) { Write-Host 'Nothing to test: no AutoCAD or Navisworks found.'; exit 2 }
$failed = @($results | Where-Object Result -eq 'FAIL')
Write-Host ("{0} passed, {1} failed" -f ($results.Count - $failed.Count), $failed.Count)
if ($failed.Count) { exit 1 } else { exit 0 }
