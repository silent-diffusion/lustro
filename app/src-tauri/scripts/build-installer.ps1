# Builds the Windows installer as target\release\bundle\nsis\Lustro-Setup-<version>.exe.
#
# 1. Always rebuilds the elevated fast-scan helper first and copies it to binaries/ as a Tauri
#    sidecar (overwriting any older copy), so the installer puts lustro-scan-helper.exe next to the
#    app exe. build.rs hashes that same file into the app (LUSTRO_HELPER_SHA256); the app refuses to
#    launch a helper with any other hash. check-helper-hash.ps1 verifies this at the end.
# 2. Builds the app and the NSIS installer (tauri.installer.conf.json adds the sidecar).
# 3. Renames Tauri's "<productName>_<version>_<arch>-setup.exe" to the release name, so there is a
#    single installer.
#
# The build never signs and never sees the updater's private key: every build script, proc macro
# and npm tool runs during it, so it gets `--no-sign` (createUpdaterArtifacts stays on in
# tauri.conf.json) and refuses to run while a signing key is in the environment. Release signing
# is a separate step afterwards: sign-installer.ps1.
#
# Usage (from anywhere):  powershell -File app/src-tauri/scripts/build-installer.ps1
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $PSScriptRoot           # app/src-tauri
$root = Resolve-Path (Join-Path $here "..\..")     # workspace root

$triple = ((rustc -vV) | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
Push-Location $root
try {
    cargo build -p lustro-scan-helper --release --locked
    if ($LASTEXITCODE -ne 0) { throw "helper build failed" }
} finally {
    Pop-Location
}
$bin = Join-Path $here "binaries"
New-Item -ItemType Directory -Force $bin | Out-Null
$sidecar = Join-Path $bin "lustro-scan-helper-$triple.exe"
Copy-Item (Join-Path $root "target\release\lustro-scan-helper.exe") $sidecar -Force
# Copy-Item keeps the source's modified time, which can be older than build.rs's last run; cargo's
# rerun-if-changed would then keep the old LUSTRO_HELPER_SHA256. Touch it so the hash is always redone.
(Get-Item $sidecar).LastWriteTime = Get-Date

# The Tauri CLI is pinned in app/ui/package.json (>= 2.12.4, which honours requireSignedVersion).
$tauri = Join-Path $here "..\ui\node_modules\.bin\tauri.cmd"
if (-not (Test-Path $tauri)) { throw "Tauri CLI not found: run 'npm ci' in app/ui first" }
foreach ($v in @("TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PATH", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD")) {
    if ([Environment]::GetEnvironmentVariable($v)) { throw "$v is set: the build must not see the signing key (sign afterwards with sign-installer.ps1)" }
}
$tauriArgs = @("build", "--bundles", "nsis", "--config", "tauri.installer.conf.json", "--no-sign")

$nsis = Join-Path $root "target\release\bundle\nsis"
# Clear earlier outputs so the folder ends up with exactly one installer.
if (Test-Path $nsis) { Get-ChildItem $nsis -File | Remove-Item -Force }

Push-Location $here
try {
    & $tauri @tauriArgs
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
} finally {
    Pop-Location
}

$conf = Get-Content (Join-Path $here "tauri.conf.json") -Raw | ConvertFrom-Json
$version = $conf.version
if (-not $version) {
    # No version in tauri.conf.json: Tauri uses the crate's (workspace) version.
    $version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
}
$built = Get-ChildItem $nsis -Filter "*_${version}_*-setup.exe" | Select-Object -First 1
if (-not $built) { throw "installer for $version not found" }
$out = Join-Path $nsis "Lustro-Setup-$version.exe"
Move-Item $built.FullName $out -Force
& (Join-Path $PSScriptRoot "check-helper-hash.ps1") -AppExe "target\release\lustro.exe"
Write-Output "Installer: target\release\bundle\nsis\Lustro-Setup-$version.exe"
