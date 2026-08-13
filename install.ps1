#Requires -Version 5.1

[CmdletBinding()]
param(
    [switch]$Quiet,
    [switch]$Force,
    [string]$Version = $env:AGENTCANPAY_VERSION,
    [string]$BinDir = $env:AGENTCANPAY_BIN_DIR
)

$ErrorActionPreference = 'Stop'
$Repo = 'imduchuyyy/agentcanpay'

if (-not $BinDir) { $BinDir = Join-Path $env:USERPROFILE '.agentcanpay\bin' }
if (-not $Force -and $env:AGENTCANPAY_IGNORE_VERIFICATION -eq 'true') {
    $Force = $true
}

function Say($msg) { if (-not $Quiet) { Write-Host "agentcanpay: $msg" } }
function Warn($msg) { Write-Warning "agentcanpay: $msg" }
function Fail($msg) { Write-Error "agentcanpay: $msg"; exit 1 }

function Get-Target {
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -eq 'ARM64') {
        # No arm64 Windows build is published; the x64 one runs under the
        # emulation layer, which is why this is a note and not an error.
        Warn 'no native arm64 build; installing the x64 build to run emulated'
    } elseif ($arch -ne 'AMD64') {
        Fail "unsupported architecture: $arch"
    }
    return 'x86_64-pc-windows-msvc'
}

function Save-Url($url, $path) {
    try {
        Invoke-WebRequest -Uri $url -OutFile $path -UseBasicParsing
        return $true
    } catch {
        return $false
    }
}

# Provenance ties the archive to the workflow run that built it; the
# checksum beside it only rules out a corrupt transfer, since anyone who
# could swap one asset could swap the other.
function Test-Download($base, $asset, $archive, $dir) {
    if ($Force) { Warn 'skipping verification of the download'; return }

    $sums = Join-Path $dir "$asset.sha256"
    if (Save-Url "$base/$asset.sha256" $sums) {
        $want = (Get-Content $sums -First 1).Split(' ')[0].Trim()
        $got = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
        if ($want -ne $got) {
            Fail "checksum mismatch for ${asset}: expected $want, got $got"
        }
        Say 'checksum ok'
    } else {
        Warn "no published checksum for $asset"
    }

    if (Get-Command gh -ErrorAction SilentlyContinue) {
        $bundle = Join-Path $dir "$asset.sigstore.json"
        if (Save-Url "$base/$asset.sigstore.json" $bundle) {
            gh attestation verify $archive --bundle $bundle --repo $Repo 2>&1 |
                Out-Null
            if ($LASTEXITCODE -ne 0) {
                Fail "provenance verification failed for $asset"
            }
            Say 'provenance verified'
        } else {
            Warn "no attestation published for $asset"
        }
    } else {
        Warn 'gh not installed, skipping provenance check (checksum only)'
    }
}

# Windows will not let a running .exe be overwritten, but it will let one be
# renamed. Moving the old binary aside first is what lets `agentcanpay
# update` replace the very binary that invoked this script.
function Install-Binary($src) {
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    $target = Join-Path $BinDir 'agentcanpay.exe'
    $stale = "$target.old"

    Remove-Item $stale -Force -ErrorAction SilentlyContinue
    if (Test-Path $target) { Move-Item $target $stale -Force }

    try {
        Move-Item $src $target -Force
    } catch {
        if (Test-Path $stale) { Move-Item $stale $target -Force }
        Fail "could not replace $target : $_"
    }
    Remove-Item $stale -Force -ErrorAction SilentlyContinue
}

$target = Get-Target
$asset = "agentcanpay-$target.zip"

if ($Version) {
    $base = "https://github.com/$Repo/releases/download/v$($Version.TrimStart('v'))"
    Say "installing agentcanpay $($Version.TrimStart('v')) for $target"
} else {
    $base = "https://github.com/$Repo/releases/latest/download"
    Say "installing the latest agentcanpay for $target"
}

$dir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $dir | Out-Null
try {
    $archive = Join-Path $dir $asset
    if (-not (Save-Url "$base/$asset" $archive)) {
        Fail "could not download $base/$asset"
    }
    Test-Download $base $asset $archive $dir

    Expand-Archive -Path $archive -DestinationPath $dir -Force
    $exe = Join-Path $dir "agentcanpay-$target\agentcanpay.exe"
    if (-not (Test-Path $exe)) { Fail "$asset did not contain a binary" }

    Install-Binary $exe
    Say "installed agentcanpay to $BinDir\agentcanpay.exe"

    if (($env:PATH -split ';') -notcontains $BinDir) {
        Say ''
        Say 'add it to your PATH:'
        Say ''
        Say "  setx PATH `"`$env:PATH;$BinDir`""
    }
} finally {
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
