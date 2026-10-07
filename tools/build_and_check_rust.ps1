<#
.SYNOPSIS
    Builds the cell viewer and runs every Rust check CI runs, in CI's order.

.DESCRIPTION
    The Rust side of the preflight (PREFLIGHT.md, section 1). Steps:

        viewer      cargo build --release in viewer-shell/        (the app the GUI launches)
        native      cargo test  native/                           (the Python module's Rust tests)
        viewcore    cargo test  viewer-shell/viewcore/            (the viewer's engine)
        luacore     cargo test  viewer-shell/luacore/             (OpenMW Lua, shared by both)
        commands    cargo build viewer-shell/check-commands/      (the viewer's commands, without Tauri)
        check       cargo build viewer-shell/check/
        clippy      cargo clippy on native, luacore, viewcore, check-commands and the viewer
                    (a warning fails it, as in CI; -ClippyReport only lists them)
        pages       wg-view-serve + node boot.js, cell and mesh   (only with -Pages; needs Node 22)

    Every step runs even if an earlier one fails, and a table at the end says which
    passed. The exit code is 1 if any failed, so it can gate a push.

    This does not install the Python module into your venvs -- that is
    tools\setup_dev_env.ps1 (-NativeOnly after changing native/).

.PARAMETER Pages
    Also run the viewer page boot tests. Needed after changing viewer-shell/ui/ or
    the viewer's commands.

.PARAMETER SkipViewer
    Skip the release build of the viewer (the slowest step) when you only want the checks.

.PARAMETER ClippyReport
    List clippy's warnings without failing the step (every lint capped at a warning).
    By default the step is strict (-D warnings), as CI's is.

.PARAMETER Only
    Run just the named steps, e.g. -Only viewcore,native

.EXAMPLE
    .\tools\build_and_check_rust.ps1
    Build the viewer and run the Rust checks.

.EXAMPLE
    .\tools\build_and_check_rust.ps1 -Pages
    The same, plus the page boot tests.
#>
[CmdletBinding()]
param(
    [switch]$Pages,
    [switch]$SkipViewer,
    [switch]$ClippyReport,
    [ValidateSet('viewer', 'native', 'viewcore', 'luacore', 'commands', 'check', 'clippy', 'pages')]
    [string[]]$Only
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

# Node and cargo print UTF-8; Windows PowerShell decodes program output with the
# console's code page (437/850), which garbles anything non-ASCII. Read it as UTF-8.
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Rust is not installed (no 'cargo' on PATH). Install it from https://rustup.rs, open a new window, and run this again."
}

function Invoke-Native {
    # Run a program; return $true when it exits 0. Its output goes to the console
    # (Out-Host), not into this function's return value.
    param([string]$Exe, [string[]]$Arguments)
    & $Exe @Arguments | Out-Host
    return ($LASTEXITCODE -eq 0)
}

function Invoke-PageTests {
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        Write-Host "  Node is not installed (needs Node 22)" -ForegroundColor Red
        return $false
    }
    $ok = Invoke-Native cargo @('build', '--release', '--manifest-path', 'viewer-shell/check-commands/Cargo.toml', '--bin', 'wg-view-serve')
    if (-not $ok) { return $false }
    Push-Location 'viewer-shell/ui/tests'
    try {
        if (-not (Invoke-Native npm @('install', '--no-audit', '--no-fund'))) { return $false }
        Write-Host "  - the Editor's windows (editor/*.test.js)" -ForegroundColor Cyan
        if (-not (Invoke-Native node @('editor/run.js'))) { return $false }
        Write-Host "  - cell viewer page" -ForegroundColor Cyan
        if (-not (Invoke-Native node @('boot.js'))) { return $false }
        Write-Host "  - mesh viewer page" -ForegroundColor Cyan
        $env:WG_MESH_VIEW = '1'
        try {
            return (Invoke-Native node @('boot.js'))
        } finally {
            Remove-Item Env:WG_MESH_VIEW -ErrorAction SilentlyContinue
        }
    } finally {
        Pop-Location
    }
}

function Invoke-Clippy {
    # Our own crates only (the forks and dependencies are not linted). Strict by default,
    # as CI is; -ClippyReport caps every lint at a warning so the step lists them and
    # passes. The lints this code base allows are in each crate's [lints.clippy].
    $lint = $(if ($ClippyReport) { @('--cap-lints', 'warn') } else { @('-D', 'warnings') })
    $ok = $true
    foreach ($m in @('native/Cargo.toml', 'viewer-shell/luacore/Cargo.toml', 'viewer-shell/viewcore/Cargo.toml',
                     'viewer-shell/check-commands/Cargo.toml', 'viewer-shell/Cargo.toml')) {
        Write-Host "  - clippy $m" -ForegroundColor Cyan
        if (-not (Invoke-Native cargo (@('clippy', '--all-targets', '--manifest-path', $m, '--') + $lint))) { $ok = $false }
    }
    return $ok
}

$steps = [ordered]@{
    viewer   = @{ Label = 'viewer release build';        Run = { Invoke-Native cargo @('build', '--release', '--manifest-path', 'viewer-shell/Cargo.toml') } }
    native   = @{ Label = 'native tests';                Run = { Invoke-Native cargo @('test', '--manifest-path', 'native/Cargo.toml') } }
    viewcore = @{ Label = 'viewcore tests';              Run = { Invoke-Native cargo @('test', '--manifest-path', 'viewer-shell/viewcore/Cargo.toml') } }
    luacore  = @{ Label = 'luacore tests';               Run = { Invoke-Native cargo @('test', '--manifest-path', 'viewer-shell/luacore/Cargo.toml') } }
    commands = @{ Label = 'check-commands build';        Run = { Invoke-Native cargo @('build', '--manifest-path', 'viewer-shell/check-commands/Cargo.toml') } }
    check    = @{ Label = 'check build';                 Run = { Invoke-Native cargo @('build', '--manifest-path', 'viewer-shell/check/Cargo.toml') } }
    clippy   = @{ Label = $(if ($ClippyReport) { 'clippy (report)' } else { 'clippy' }); Run = { Invoke-Clippy } }
    pages    = @{ Label = 'page boot tests (cell, mesh)'; Run = { Invoke-PageTests } }
}

if ($Only) {
    $wanted = $Only
} else {
    $wanted = @('viewer', 'native', 'viewcore', 'luacore', 'commands', 'check', 'clippy')
    if ($SkipViewer) { $wanted = $wanted | Where-Object { $_ -ne 'viewer' } }
    if ($Pages) { $wanted += 'pages' }
}

# Cargo reports progress on stderr; don't let Windows PowerShell treat that as an error.
$ErrorActionPreference = 'Continue'

$results = @()
foreach ($name in $steps.Keys) {
    if ($wanted -notcontains $name) { continue }
    $step = $steps[$name]
    Write-Host ""
    Write-Host "== $($step.Label) ==" -ForegroundColor Yellow
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    $ok = [bool](& $step.Run)
    $timer.Stop()
    $results += [pscustomobject]@{
        Step   = $step.Label
        Result = $(if ($ok) { 'passed' } else { 'FAILED' })
        Time   = '{0:mm\:ss}' -f $timer.Elapsed
    }
}

Write-Host ""
$results | Format-Table -AutoSize
$failed = @($results | Where-Object { $_.Result -eq 'FAILED' })
if ($failed.Count -gt 0) {
    Write-Host "$($failed.Count) step(s) failed -- scroll up for the first error." -ForegroundColor Red
    exit 1
}
Write-Host "All Rust steps passed." -ForegroundColor Green
