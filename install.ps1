# Install `quant` from this repo root (Claude-Code style: one command, then `quant` works anywhere).
# Usage: .\install.ps1
$ErrorActionPreference = 'Stop'

function Fail($msg) { Write-Error $msg; exit 1 }

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Fail "cargo not found. Install Rust 1.85+ from https://rustup.rs first."
}

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

Write-Host "Installing quant (release)..."
cargo install --locked --path . --force
if ($LASTEXITCODE -ne 0) { Fail "cargo install failed." }

$ver = & quant --version 2>$null
if ($LASTEXITCODE -ne 0 -or -not $ver) {
  Fail "installed but 'quant' is not on PATH. Add '$env:USERPROFILE\.cargo\bin' to PATH, then reopen the terminal."
}

Write-Host "Done: $ver"
Write-Host 'Try: quant show --input data/sample.csv --limit 3'
