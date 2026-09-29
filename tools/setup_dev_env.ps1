<#
.SYNOPSIS
    Sets up the dev environments and builds the Rust backend (wraithguard_native) into each.

.DESCRIPTION
    Two environments, because the regular and the free-threaded Python share one
    site-packages on Windows and a native module built for one does not load in the other:

        .venv-t   Python 3.14t (free-threaded, what the app ships)   required
        .venv     Python 3.14  (CI's other interpreter)               skipped if 3.14 is not installed

    For each: creates the venv if it is missing, installs the project with its dev
    tools (pip install -e .[dev]), rebuilds native/, then imports the module and checks the GIL state.

    Run it from anywhere; it works from the repo folder. Safe to run again at any time.

.PARAMETER NativeOnly
    Only rebuild native/ in the venvs that already exist. Use this after changing native/.

.PARAMETER FreeThreadedOnly
    Only touch .venv-t.

.EXAMPLE
    .\tools\setup_dev_env.ps1
    First-time setup, or a full refresh.

.EXAMPLE
    .\tools\setup_dev_env.ps1 -NativeOnly
    Rebuild the Rust backend after editing native/.
#>
[CmdletBinding()]
param(
    [switch]$NativeOnly,
    [switch]$FreeThreadedOnly
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

function Invoke-Step {
    # Run a native command and stop on a non-zero exit code, which
    # $ErrorActionPreference does not catch for .exe programs.
    param([string]$What, [scriptblock]$Command)
    Write-Host "  - $What" -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$What failed (exit code $LASTEXITCODE)."
    }
}

# --- Prerequisites -----------------------------------------------------------

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Rust is not installed (no 'cargo' on PATH). Install it from https://rustup.rs, open a new window, and run this again."
}
if (-not $NativeOnly -and -not (Get-Command py -ErrorAction SilentlyContinue)) {
    throw "The Python launcher 'py' is not on PATH. Install Python 3.14 from python.org (or the Python install manager) and run this again."
}

$envs = @(
    @{ Dir = '.venv-t'; Version = '3.14t'; Gil = 'False'; Required = $true },
    @{ Dir = '.venv';   Version = '3.14';  Gil = 'True';  Required = $false }
)
if ($FreeThreadedOnly) { $envs = @($envs[0]) }

$results = @()

foreach ($e in $envs) {
    $python = Join-Path $repo (Join-Path $e.Dir 'Scripts\python.exe')
    Write-Host ""
    Write-Host "== $($e.Dir) (Python $($e.Version)) ==" -ForegroundColor Yellow

    if (-not (Test-Path $python)) {
        if ($NativeOnly) {
            Write-Host "  not created yet; skipped (run without -NativeOnly to create it)" -ForegroundColor DarkYellow
            $results += [pscustomobject]@{ Env = $e.Dir; Result = 'skipped: no venv' }
            continue
        }
        # Windows PowerShell turns a native program's stderr into an error when
        # redirected, which 'Stop' would throw on; relax it for this probe.
        $ErrorActionPreference = 'Continue'
        & py "-$($e.Version)" -c "pass" 2>$null
        $found = ($LASTEXITCODE -eq 0)
        $ErrorActionPreference = 'Stop'
        if (-not $found) {
            if ($e.Required) {
                throw "Python $($e.Version) is not installed. Install it (py install $($e.Version), or the free-threaded option in the python.org installer) and run this again."
            }
            Write-Host "  Python $($e.Version) is not installed; skipped" -ForegroundColor DarkYellow
            $results += [pscustomobject]@{ Env = $e.Dir; Result = "skipped: no Python $($e.Version)" }
            continue
        }
        Invoke-Step "create $($e.Dir)" { & py "-$($e.Version)" -m venv $e.Dir }
    }

    if (-not $NativeOnly) {
        Invoke-Step 'upgrade pip' { & $python -m pip install --quiet --upgrade pip }
        Invoke-Step 'install the project and dev tools (-e .[dev])' { & $python -m pip install --quiet -e '.[dev]' }
    }

    Invoke-Step 'build and install native/ (takes a minute or two)' {
        & $python -m pip install --force-reinstall --no-deps ./native
    }

    # Import it from outside the repo folder, so the check finds the installed
    # module and not anything lying around in the working directory.
    $check = 'import sys, wraithguard_native as n; print(n.__version__); print(sys._is_gil_enabled())'
    Push-Location $env:TEMP
    $ErrorActionPreference = 'Continue'
    try {
        $out = & $python -c $check 2>&1
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
        Pop-Location
    }
    if ($code -ne 0) {
        throw "wraithguard_native was built but does not import in $($e.Dir):`n$out"
    }
    $version = ($out | Select-Object -First 1).ToString().Trim()
    $gil = ($out | Select-Object -Last 1).ToString().Trim()
    if ($gil -ne $e.Gil) {
        Write-Host "  warning: GIL enabled = $gil, expected $($e.Gil). Something in this venv turned the GIL back on." -ForegroundColor DarkYellow
    }
    Write-Host "  wraithguard_native $version loads (GIL enabled: $gil)" -ForegroundColor Green
    $results += [pscustomobject]@{ Env = $e.Dir; Result = "ok: native $version, GIL $gil" }
}

Write-Host ""
$results | Format-Table -AutoSize
Write-Host "Activate the main one with:  .venv-t\Scripts\Activate.ps1"
