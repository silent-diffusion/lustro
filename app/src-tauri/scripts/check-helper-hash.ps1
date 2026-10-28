# Fails unless the fast-scan helper next to the app is the one whose SHA-256 the app pins.
#
# build.rs bakes the hash of binaries/lustro-scan-helper-<triple>.exe into the app as
# LUSTRO_HELPER_SHA256 (a 64-hex-digit string in the exe). This checks that:
#   1. the sidecar copy in binaries/ is byte-identical to the freshly built
#      target/release/lustro-scan-helper.exe (so it isn't stale), and
#   2. each given app exe contains exactly that hash.
# A mismatch means the app would refuse to launch the helper, so Fast scan would silently fall
# back to the walker.
#
# Usage:  powershell -File app/src-tauri/scripts/check-helper-hash.ps1 -AppExe target\release\lustro.exe [, more.exe]
param([Parameter(Mandatory)][string[]]$AppExe)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $PSScriptRoot           # app/src-tauri
$root = Resolve-Path (Join-Path $here "..\..")     # workspace root

$triple = ((rustc -vV) | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
$built = Join-Path $root "target\release\lustro-scan-helper.exe"
$sidecar = Join-Path $here "binaries\lustro-scan-helper-$triple.exe"
foreach ($f in @($built, $sidecar)) { if (-not (Test-Path $f)) { throw "$f not found" } }

$builtHash = (Get-FileHash $built -Algorithm SHA256).Hash.ToLowerInvariant()
$sidecarHash = (Get-FileHash $sidecar -Algorithm SHA256).Hash.ToLowerInvariant()
if ($builtHash -ne $sidecarHash) {
    throw "binaries/ helper ($sidecarHash) differs from the fresh build ($builtHash): it is stale"
}

foreach ($exe in $AppExe) {
    $path = if ([System.IO.Path]::IsPathRooted($exe)) { $exe } else { Join-Path $root $exe }
    $text = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($path))
    if (-not $text.Contains($builtHash)) { throw "$exe doesn't pin the helper hash $builtHash (rebuild the app after the helper)" }
    Write-Output "OK  $exe pins lustro-scan-helper $builtHash"
}
