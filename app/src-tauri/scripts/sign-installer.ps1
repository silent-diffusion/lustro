# Signs the installer for the updater: writes <installer>.sig.
#
# In the release workflow this runs in its own job (`sign`, environment `release`) on a fresh
# runner that has only this script, the unsigned installer artifact and the Tauri CLI fetched by
# fetch-tauri-cli.mjs, so nothing from the build can reach the private key. With the NSIS updater,
# the installer itself is the update package, so it is the only file that needs a signature.
#
# Needs TAURI_SIGNING_PRIVATE_KEY in the environment, plus TAURI_SIGNING_PRIVATE_KEY_PASSWORD if
# the key has a password.
# The signature is bound to the version (--app-version), as requireSignedVersion requires.
#
# Usage:  pwsh sign-installer.ps1 -Installer <Lustro-Setup-x.y.z.exe> -Version x.y.z [-TauriJs <tauri.js>]
#         (-TauriJs defaults to app/ui/node_modules/@tauri-apps/cli/tauri.js)
param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$Version,
    [string]$TauriJs
)
$ErrorActionPreference = "Stop"

if (-not $env:TAURI_SIGNING_PRIVATE_KEY) { throw "TAURI_SIGNING_PRIVATE_KEY is not set" }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "version '$Version' is not x.y.z" }
if (-not $TauriJs) {
    $TauriJs = Join-Path $PSScriptRoot "../../ui/node_modules/@tauri-apps/cli/tauri.js"
}
if (-not (Test-Path $TauriJs)) { throw "Tauri CLI not found at $TauriJs" }
if (-not (Test-Path $Installer)) { throw "$Installer not found" }
$Installer = (Resolve-Path $Installer).Path
$before = (Get-FileHash $Installer -Algorithm SHA256).Hash

$sig = "$Installer.sig"
if (Test-Path $sig) { Remove-Item $sig -Force }
& node $TauriJs signer sign --app-version $Version $Installer
if ($LASTEXITCODE -ne 0) { throw "signing failed" }

# The release must not go out without a version-bound signature over these exact bytes.
if (-not (Test-Path $sig)) { throw "$sig was not created" }
if ((Get-FileHash $Installer -Algorithm SHA256).Hash -ne $before) { throw "the installer changed while signing" }
$decoded = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String((Get-Content $sig -Raw).Trim()))
if ($decoded -notmatch ('version:\s*' + [regex]::Escape($Version))) {
    throw "the signature isn't bound to version $Version"
}
Write-Output "Signed: $sig"
