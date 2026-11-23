<#
.SYNOPSIS
    Runs the Lua regression corpus: the Lua mods viewer-shell/viewcore/tests/lua_corpus/
    expected.json lists, from your own copy of them, through the Preview's Lua.

.DESCRIPTION
    Static 2026-11-23 ("a small regression corpus"). The corpus is a list of mods by path under
    a folder of your own (no mod is in the repository: they are other people's work), each with
    the errors the Preview's Lua gives for it when it runs on its own. A new error fails the run
    and is named; one that went away is said ("fixed"). After a change that fixes errors on
    purpose, -Bless writes what the mods say now as the expected.

    Your copy is the folder robocopy made of the mods' Lua (see the corpus file's note):
        robocopy "E:\OpenMW\Mods" "<subset sort>\omw-lua" *.lua *.omwscripts *.yaml /S

.PARAMETER Corpus
    The folder the corpus' paths are under. Default: omw-lua beside the repository.

.PARAMETER Resources
    OpenMW's resources folder (its built-in scripts). Default: E:\OpenMW\resources.

.PARAMETER Bless
    Write the current errors as the expected (the file changes; review it before committing).

.EXAMPLE
    .\tools\lua_corpus.ps1
#>
[CmdletBinding()]
param(
    [string]$Corpus = '',
    [string]$Resources = 'E:\OpenMW\resources',
    [switch]$Bless
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
if (-not $Corpus) { $Corpus = Join-Path (Split-Path -Parent $repo) 'omw-lua' }
if (-not (Test-Path $Corpus)) { throw "No corpus folder at '$Corpus' (see -Corpus)." }
if (-not (Test-Path $Resources)) { Write-Host "No OpenMW resources at '$Resources': the mods run without OpenMW's built-ins (see -Resources)." -ForegroundColor Yellow }

$env:WG_LUA_CORPUS = (Resolve-Path $Corpus).Path
if (Test-Path $Resources) { $env:WG_OMW_RES = (Resolve-Path $Resources).Path }
if ($Bless) { $env:WG_LUA_CORPUS_BLESS = '1' }
$ErrorActionPreference = 'Continue'
try {
    Push-Location (Join-Path $repo 'viewer-shell')
    & cargo test -p viewcore --lib lua_corpus -- --ignored --nocapture | Out-Host
    $code = $LASTEXITCODE
} finally {
    Pop-Location
    Remove-Item Env:WG_LUA_CORPUS, Env:WG_OMW_RES, Env:WG_LUA_CORPUS_BLESS -ErrorAction SilentlyContinue
}
if ($code -ne 0) { Write-Host "The corpus has new errors (NEW above)." -ForegroundColor Red; exit 1 }
Write-Host $(if ($Bless) { 'Expected written: review viewer-shell/viewcore/tests/lua_corpus/expected.json.' } else { 'The corpus says what it is expected to.' }) -ForegroundColor Green
