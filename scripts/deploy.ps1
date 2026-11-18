[CmdletBinding()]
param(
    [switch]$SkipChecks,
    [switch]$Push,
    [switch]$Tag,
    [string]$Version
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $repoRoot
try {
    if ($Version) {
        if ($Version -notmatch '^\d+\.\d+\.\d+$') {
            throw "Invalid semantic version: '$Version' (expected e.g. 0.2.0)"
        }
        $tomlPath = Join-Path $repoRoot "Cargo.toml"
        $toml = Get-Content $tomlPath -Raw -Encoding UTF8
        $updated = [regex]::Replace(
            $toml,
            '(?m)^version\s*=\s*"[^"]+"',
            "version = `"$Version`"",
            1
        )
        [System.IO.File]::WriteAllText($tomlPath, $updated, (New-Object System.Text.UTF8Encoding($false)))
        cargo check --quiet
    }

    $cargoToml = Get-Content (Join-Path $repoRoot "Cargo.toml") -Raw
    if ($cargoToml -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
        throw "Could not read version from Cargo.toml"
    }
    $pkgVersion = $Matches[1]
    Write-Host "==> Deploying quant v$pkgVersion from $repoRoot" -ForegroundColor Cyan

    if (-not $SkipChecks) {
        Write-Host "==> Running cargo test --locked" -ForegroundColor Cyan
        cargo test --locked
        if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

        Write-Host "==> Running strict cargo clippy" -ForegroundColor Cyan
        cargo clippy --all-targets -- -D warnings -D clippy::unwrap_used -D clippy::expect_used
        if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }
    }

    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME ".cargo" }
    $binDir = Join-Path $cargoHome "bin"
    $binPath = Join-Path $binDir "quant.exe"
    $backupPath = $null
    if (Test-Path $binPath) {
        $backupPath = "$binPath.old-$PID"
        Write-Host "==> Moving existing binary aside -> $backupPath" -ForegroundColor DarkGray
        Move-Item -Force $binPath $backupPath
    }

    Write-Host "==> Installing release binary via cargo install --locked --path . --force" -ForegroundColor Cyan
    cargo install --locked --path . --force
    if ($LASTEXITCODE -ne 0) {
        if ($backupPath -and (Test-Path $backupPath) -and -not (Test-Path $binPath)) {
            Move-Item -Force $backupPath $binPath
        }
        throw "cargo install failed"
    }

    Get-ChildItem -Path $binDir -Filter "quant.exe.old-*" -ErrorAction SilentlyContinue | ForEach-Object {
        try { Remove-Item $_.FullName -Force -ErrorAction Stop } catch {}
    }

    & $binPath --version

    if ($Push -or $Tag) {
        Write-Host "==> Pushing branch to origin..." -ForegroundColor Cyan
        git push origin HEAD
        if ($LASTEXITCODE -ne 0) { throw "git push failed" }
    }

    if ($Tag) {
        $tagName = "v$pkgVersion"
        $existing = git tag -l $tagName
        if (-not $existing) {
            Write-Host "==> Creating tag $tagName" -ForegroundColor Cyan
            git tag $tagName
            if ($LASTEXITCODE -ne 0) { throw "git tag failed" }
        }
        Write-Host "==> Pushing tag $tagName (triggers GitHub release workflow)..." -ForegroundColor Cyan
        git push origin $tagName
        if ($LASTEXITCODE -ne 0) { throw "git push tag failed" }
    }

    Write-Host "==> Deployment complete: quant v$pkgVersion" -ForegroundColor Green
}
finally {
    Pop-Location
}
