<#
.SYNOPSIS
    Applies a patch to the working tree, checks it first, runs the quick checks for what it
    touches, and prints the suggested commit title. Nothing is committed or pushed.

.DESCRIPTION
    For the patches Claude leaves in "subset sort" (Static 2026-11-23: "an apply-and-check
    script"). Steps:

        1. git apply --check: the patch applies to this tree as it is, or nothing changes.
        2. git apply, and the files it changed listed.
        3. The quick checks for what it touches (skip with -NoChecks):
             viewer-shell/ui/          node editor/run.js, the html sinks lint
                                       (and the preview suite, npm run preview)
             viewer-shell/viewcore/    cargo test viewcore --lib
             the viewer's commands     cargo test check-commands --no-run (they compile)
           The whole of CI is still tools/build_and_check_rust.ps1 -Pages.
        4. The suggested commit title, from the patch's first lines
           ("Suggested-Commit: ..."), when it has one.

    Your index and your commits are left alone: what to commit is yours to decide.

.PARAMETER Patch
    The patch file (a path, or a name in the "subset sort" folder beside the repository).

.PARAMETER NoChecks
    Apply only.

.EXAMPLE
    .\tools\apply_patch.ps1 wg_lua_tools.patch
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Patch,
    [switch]$NoChecks
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

# The patch: as given, else beside the repository (the folder the patches are left in).
$file = $Patch
if (-not (Test-Path $file)) { $file = Join-Path (Split-Path -Parent $repo) $Patch }
if (-not (Test-Path $file)) { throw "No patch at '$Patch' (nor beside the repository)." }
$file = (Resolve-Path $file).Path
Set-Location $repo

# Native programs report on stderr; that is not an error here.
$ErrorActionPreference = 'Continue'

Write-Host "== checking $([System.IO.Path]::GetFileName($file)) ==" -ForegroundColor Yellow
& git apply --check $file
if ($LASTEXITCODE -ne 0) {
    Write-Host "The patch does not apply to this tree as it is (above: where). Nothing was changed." -ForegroundColor Red
    exit 1
}
$files = @(& git apply --numstat $file | ForEach-Object { ($_ -split "`t")[2] })
& git apply $file
if ($LASTEXITCODE -ne 0) { Write-Host "git apply failed after the check passed." -ForegroundColor Red; exit 1 }
Write-Host "Applied: $($files.Count) file(s)" -ForegroundColor Green
$files | ForEach-Object { "  $_" }

$results = @()
function Step([string]$Label, [string]$Dir, [string]$Exe, [string[]]$Arguments) {
    Write-Host ""
    Write-Host "== $Label ==" -ForegroundColor Yellow
    Push-Location $Dir
    try {
        & $Exe @Arguments | Out-Host
        $ok = ($LASTEXITCODE -eq 0)
    } finally { Pop-Location }
    $script:results += [pscustomobject]@{ Check = $Label; Result = $(if ($ok) { 'passed' } else { 'FAILED' }) }
}

if (-not $NoChecks) {
    $ui = $files | Where-Object { $_ -like 'viewer-shell/ui/*' }
    $core = $files | Where-Object { $_ -like 'viewer-shell/viewcore/*' -or $_ -like 'viewer-shell/Cargo.lock' }
    $cmds = $files | Where-Object { $_ -like 'viewer-shell/src/*' -or $_ -like 'viewer-shell/check-commands/*' -or $_ -like 'viewer-shell/viewcore/*' }
    if ($ui) {
        if (Get-Command node -ErrorAction SilentlyContinue) {
            Step 'the Editor windows (editor/run.js)' 'viewer-shell/ui/tests' 'node' @('editor/run.js')
            Step 'the preview suite' 'viewer-shell/ui/tests' 'npm' @('run', '-s', 'preview')
            Step 'no new innerHTML sinks' 'viewer-shell/ui/tests' 'node' @('html_sinks.js')
        } else { Write-Host "Node is not installed: the page checks were skipped." -ForegroundColor Red }
    }
    if ($core) { Step 'viewcore tests' 'viewer-shell' 'cargo' @('test', '-p', 'viewcore', '--lib') }
    if ($cmds) { Step 'the commands compile' 'viewer-shell' 'cargo' @('test', '-p', 'wraithguard-viewer-check-commands', '--no-run') }
}

# The suggested commit title: the patch's own, when it says one.
$title = Get-Content $file -TotalCount 20 | Where-Object { $_ -match '^Suggested-Commit:\s*(.+)$' } | Select-Object -First 1
Write-Host ""
if ($results.Count) { $results | Format-Table -AutoSize }
if ($title) { Write-Host ("Suggested commit title: " + ($title -replace '^Suggested-Commit:\s*', '')) -ForegroundColor Cyan }
Write-Host "Nothing was committed or pushed." -ForegroundColor DarkGray
if (@($results | Where-Object { $_.Result -eq 'FAILED' }).Count) { exit 1 }
