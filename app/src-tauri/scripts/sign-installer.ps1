# Signs the installer for the updater: writes target\release\bundle\nsis\Lustro-Setup-<v>.exe.sig.
#
# Runs after build-installer.ps1, as its own release-workflow step, so the private key is only in
# the environment of this script and the pinned Tauri CLI (app/ui/node_modules), never of the
# build's dependencies. With the NSIS updater, the installer itself is the update package, so it
# is the only file that needs a signature.
#
# Needs TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD in the environment.
# The signature is bound to the version (--app-version), as requireSignedVersion requires.
#
# Usage:  powershell -File app/src-tauri/scripts/sign-installer.ps1 [-Version x.y.z]
param([string]$Version)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $PSScriptRoot           # app/src-tauri
$root = Resolve-Path (Join-Path $here "..\..")     # workspace root

if (-not $env:TAURI_SIGNING_PRIVATE_KEY) { throw "TAURI_SIGNING_PRIVATE_KEY is not set" }
if (-not $Version) {
    $Version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
}
$tauri = Join-Path $here "..\ui\node_modules\.bin\tauri.cmd"
if (-not (Test-Path $tauri)) { throw "Tauri CLI not found: run 'npm ci' in app/ui first" }

$setup = Join-Path $root "target\release\bundle\nsis\Lustro-Setup-$Version.exe"
if (-not (Test-Path $setup)) { throw "Lustro-Setup-$Version.exe not found; run build-installer.ps1 first" }
$sig = "$setup.sig"
if (Test-Path $sig) { Remove-Item $sig -Force }

& $tauri signer sign --app-version $Version $setup
if ($LASTEXITCODE -ne 0) { throw "signing failed" }

# The release must not go out without a version-bound signature.
if (-not (Test-Path $sig)) { throw "Lustro-Setup-$Version.exe.sig was not created" }
$decoded = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String((Get-Content $sig -Raw).Trim()))
if ($decoded -notmatch ('version:\s*' + [regex]::Escape($Version))) {
    throw "the signature isn't bound to version $Version"
}
Write-Output "Signed: target\release\bundle\nsis\Lustro-Setup-$Version.exe.sig"
