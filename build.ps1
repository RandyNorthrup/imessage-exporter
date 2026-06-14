#!/usr/bin/env pwsh
# Check, test, build, and package the release binaries.
#
# This is the native-Windows (PowerShell) counterpart to build.sh and performs
# the exact same steps. It also runs on macOS and Linux under PowerShell Core,
# so either script can be used on any platform.

$ErrorActionPreference = 'Stop'
Set-Location -Path $PSScriptRoot

if (-not $env:CARGO_TERM_COLOR) { $env:CARGO_TERM_COLOR = 'always' }

# Native commands (cargo, rustc) do not throw on a non-zero exit, so each step
# checks $LASTEXITCODE explicitly and aborts the whole script on failure.
function Invoke-Step {
    param(
        [Parameter(Mandatory)] [string] $Message,
        [Parameter(Mandatory)] [scriptblock] $Command
    )
    Write-Host $Message
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Step failed (exit $LASTEXITCODE): $Message"
    }
}

Invoke-Step 'Checking formatting...' { cargo fmt --all --check }
Invoke-Step 'Running clippy...' { cargo clippy --workspace --all-targets -- -D warnings }

if (-not $env:RUSTDOCFLAGS) { $env:RUSTDOCFLAGS = '-D warnings' }
Invoke-Step 'Building documentation...' { cargo doc --no-deps --workspace }

Invoke-Step 'Running tests...' { cargo test --workspace --all-targets }
Invoke-Step 'Building release binaries...' {
    cargo build --release -p imessage-exporter -p imessage-gui
}

# Host target triple (e.g. x86_64-pc-windows-msvc) used to name the artifacts.
$hostLine = rustc -vV | Select-String '^host:'
if ($LASTEXITCODE -ne 0 -or -not $hostLine) {
    throw 'Could not determine the host target triple from rustc.'
}
$target = $hostLine.Line.Split(' ')[1]

$binExt = if ($target -like '*windows*') { '.exe' } else { '' }

New-Item -ItemType Directory -Force -Path 'output' | Out-Null
Copy-Item "target/release/imessage-exporter$binExt" "output/imessage-exporter-$target$binExt" -Force
Copy-Item "target/release/imessage-gui$binExt" "output/imessage-gui-$target$binExt" -Force
Copy-Item 'LICENSE' 'output/' -Force

Write-Host 'Built:'
Write-Host "  output/imessage-exporter-$target$binExt"
Write-Host "  output/imessage-gui-$target$binExt"
Write-Host '  output/LICENSE'
