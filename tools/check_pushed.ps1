<#
.SYNOPSIS
    Says whether what CI will test is what you tested locally.

.DESCRIPTION
    Read-only (no index lock): fetches, then lists
      - the branch you are on,
      - changes not staged, and changes staged but not committed (neither is pushed),
      - commits on this branch that origin/main and origin/build/test don't have,
      - files whose committed copy differs from your working copy,
    and prints the commit SHA to look for at the top of the GitHub Actions run.

.EXAMPLE
    .\tools\check_pushed.ps1
#>
[CmdletBinding()]
param([string[]]$Remotes = @('main', 'build/test'))

$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo
$git = @('--no-optional-locks')

& git @git fetch --quiet origin
$branch = (& git @git rev-parse --abbrev-ref HEAD).Trim()
$head = (& git @git rev-parse --short HEAD).Trim()
Write-Host "On branch: $branch at $head" -ForegroundColor Cyan

$unstaged = & git @git diff --name-only
$staged = & git @git diff --cached --name-only
if ($unstaged) { Write-Host "`nChanged, NOT staged (not in any commit):" -ForegroundColor Red; $unstaged | ForEach-Object { "  $_" } }
if ($staged) { Write-Host "`nStaged, NOT committed (not pushed):" -ForegroundColor Red; $staged | ForEach-Object { "  $_" } }
$untracked = & git @git ls-files --others --exclude-standard -- '*.py' '*.js' '*.rs' '*.toml' '*.yml'
if ($untracked) { Write-Host "`nNew files git doesn't track yet:" -ForegroundColor Red; $untracked | ForEach-Object { "  $_" } }

foreach ($r in $Remotes) {
    $ahead = & git @git log --oneline "origin/$r..HEAD" 2>$null
    $remoteHead = (& git @git rev-parse --short "origin/$r" 2>$null)
    if ($LASTEXITCODE -ne 0) { Write-Host "`norigin/$r does not exist" -ForegroundColor Yellow; continue }
    if ($ahead) {
        Write-Host "`norigin/$r is at $remoteHead and is MISSING these commits:" -ForegroundColor Red
        $ahead | ForEach-Object { "  $_" }
    } else {
        Write-Host "`norigin/$r is at $remoteHead - has everything committed here" -ForegroundColor Green
    }
}

if (-not ($unstaged -or $staged -or $untracked)) {
    Write-Host "`nWorking copy matches the last commit." -ForegroundColor Green
}
Write-Host "`nCI tests commit $head only if its run shows that SHA." -ForegroundColor Cyan
