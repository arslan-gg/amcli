# Install the amcli binary on Windows.
#
# CONTRACT, matching scripts/install.sh: stdout carries the absolute path of
# the binary and nothing else, so an agent can rely on
#
#     $AMCLI = & ~\.agents\skills\amcli\scripts\install.ps1
#
# and use $AMCLI for the rest of the session. Everything a human reads goes to
# stderr, because a freshly installed binary is not on PATH until the shell is
# restarted, and "amcli" alone will still report that it is not recognised.
#
# Never elevates. Never edits the registry or a profile script.
#
#   -Version v0.1.0   install that tag instead of the newest
#   -InstallDir PATH  default %LOCALAPPDATA%\Programs\amcli
#   -DryRun           report what would happen, download nothing

[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\amcli",
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # the progress bar corrupts a non-interactive host

$Repo = 'arslan-gg/amcli'
$Base = "https://github.com/$Repo"

function Say([string]$m) { [Console]::Error.WriteLine($m) }
function Die([string]$m) { [Console]::Error.WriteLine("amcli install: $m"); exit 1 }

# Windows on ARM runs x64 binaries under emulation, and there is no arm64
# build yet, so x64 is the right answer for both.
$target = 'x86_64-pc-windows-msvc'
$bin = 'amcli.exe'

# Resolve the newest tag from the redirect rather than api.github.com, whose
# unauthenticated limit of 60 requests/hour is shared across a whole NAT.
function Resolve-Tag {
    try {
        $r = Invoke-WebRequest -Uri "$Base/releases/latest" -MaximumRedirection 5 -UseBasicParsing
        $url = $r.BaseResponse.RequestMessage.RequestUri.AbsoluteUri
    } catch {
        return $null
    }
    $tag = $url.Split('/')[-1]
    # With no releases published GitHub lands on the list page instead, so the
    # last segment is "releases". That is a fresh repository, not an error.
    if ($tag -match '^v[0-9]') { return $tag }
    return $null
}

if ($Version) { $tag = $Version } else { $tag = Resolve-Tag }

if (-not $tag) {
    if ($DryRun) {
        Say "target:  $target"
        Say "release: none published yet, would build with cargo"
        Write-Output (Join-Path $InstallDir $bin)
        exit 0
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Die @"
no published release yet and cargo is not installed. Install Rust from
https://rustup.rs and re-run this script, or run:
  cargo install --git $Base --locked amcli-cli
"@
    }
    Say 'no published release found; building from source with cargo (a few minutes)...'
    $stage = Join-Path $InstallDir ".amcli-install-$([System.Guid]::NewGuid().ToString('N'))"
    cargo install --git $Base --locked --root $stage amcli-cli 1>&2
    if ($LASTEXITCODE -ne 0) {
        Die 'cargo could not build amcli. It needs Rust 1.90 or newer (edition 2024); run `rustup update` if yours is older.'
    }
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Move-Item -Force (Join-Path $stage "bin\$bin") (Join-Path $InstallDir $bin)
    Remove-Item -Recurse -Force $stage
    $resolved = (Resolve-Path (Join-Path $InstallDir $bin)).Path
    Say "installed $(& $resolved --version)"
    Write-Output $resolved
    exit 0
}

$asset = "amcli-$tag-$target.tar.gz"
$url = "$Base/releases/download/$tag/$asset"

if ($DryRun) {
    Say "target:  $target"
    Say "release: $tag"
    Say "url:     $url"
    Say "install: $(Join-Path $InstallDir $bin)"
    Write-Output (Join-Path $InstallDir $bin)
    exit 0
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
# Staged inside the install directory so the final move is a rename on the
# same volume and two installs cannot interleave.
$tmp = Join-Path $InstallDir ".amcli-install-$([System.Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    Say "downloading amcli $tag for $target..."
    try {
        Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmp $asset) -UseBasicParsing
        Invoke-WebRequest -Uri "$Base/releases/download/$tag/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS') -UseBasicParsing
    } catch {
        Die "could not download $asset from release $tag ($($_.Exception.Message))"
    }

    $want = (Get-Content (Join-Path $tmp 'SHA256SUMS') |
        Where-Object { ($_ -split '\s+')[1] -eq $asset } |
        ForEach-Object { ($_ -split '\s+')[0] } |
        Select-Object -First 1)
    if (-not $want) { Die "$asset is not listed in SHA256SUMS" }

    $got = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $tmp $asset)).Hash
    if ($got -ne $want.ToUpperInvariant()) {
        Die "checksum mismatch for $asset`n  expected $want`n  got      $got"
    }

    # tar.exe has shipped in Windows since 10 1803, so the release uses one
    # archive format for every platform.
    tar -xzf (Join-Path $tmp $asset) -C $tmp
    if ($LASTEXITCODE -ne 0) { Die "could not unpack $asset" }
    if (-not (Test-Path (Join-Path $tmp $bin))) { Die "$asset does not contain $bin" }

    Move-Item -Force (Join-Path $tmp $bin) (Join-Path $InstallDir $bin)
} finally {
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
}

$resolved = (Resolve-Path (Join-Path $InstallDir $bin)).Path

$onPath = (Get-Command amcli -ErrorAction SilentlyContinue).Source
if ($onPath -and $onPath -ne $resolved) {
    Say "warning: $onPath comes earlier in PATH and will shadow this install."
    Say '         Use the absolute path below, or remove the other copy.'
} elseif (-not $onPath) {
    Say "note: $InstallDir is not on PATH. Use the absolute path below, or add it:"
    Say "         setx PATH `"$InstallDir;`$env:PATH`""
}

Say "installed $(& $resolved --version)"
Write-Output $resolved
