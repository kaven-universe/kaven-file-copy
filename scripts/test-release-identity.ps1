param(
    [Parameter(Mandatory = $true)]
    [string]$Tag
)

$ErrorActionPreference = "Stop"

if ($Tag -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
    throw "Release tag must use the exact X.Y.Z format: $Tag"
}

$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$package = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot "apps\desktop\package.json") | ConvertFrom-Json
$packageLock = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot "apps\desktop\package-lock.json")
$tauri = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot "apps\desktop\src-tauri\tauri.conf.json") | ConvertFrom-Json
$cargoMetadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$rustVersions = @($cargoMetadata.packages.version | Sort-Object -Unique)
$packageLockVersions = [regex]::Matches($packageLock, '(?m)^\s*"version"\s*:\s*"([^"]+)"')
if ($packageLockVersions.Count -lt 2) { throw "package-lock.json does not contain the expected root versions." }
$versions = @($rustVersions) + @($package.version, $packageLockVersions[0].Groups[1].Value, $packageLockVersions[1].Groups[1].Value, $tauri.version)
if (@($versions | Where-Object { $_ -cne $Tag }).Count -ne 0) {
    throw "Release versions do not all match $Tag`: $($versions -join ', ')."
}
if ($tauri.identifier -cne "xyz.kaven.app.copy.file") { throw "Tauri identifier must be xyz.kaven.app.copy.file." }
if (-not $tauri.bundle.createUpdaterArtifacts) { throw "Tauri updater artifacts must be enabled." }
$endpoint = @($tauri.plugins.updater.endpoints)
if ($endpoint.Count -ne 1 -or $endpoint[0] -cne 'https://public.data.kaven.xyz/json/kaven-file-copy/latest.json') {
    throw "The canonical updater endpoint is invalid."
}
$releaseNotes = Join-Path $workspaceRoot "docs\releases\$Tag.md"
if (-not (Test-Path -LiteralPath $releaseNotes -PathType Leaf)) { throw "Reviewed release notes are missing: $releaseNotes" }

Write-Host "Release identity is valid for $Tag."
