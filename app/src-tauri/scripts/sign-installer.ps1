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
# -PubKey is the committed plugins.updater.pubkey: the signature must come from that key, or every
# installed copy would reject the update.
#
# Usage:  pwsh sign-installer.ps1 -Installer <Lustro-Setup-x.y.z.exe> -Version x.y.z -PubKey <pubkey>
#         [-TauriJs <tauri.js>]  (defaults to app/ui/node_modules/@tauri-apps/cli/tauri.js)
param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$PubKey,
    [string]$TauriJs
)
$ErrorActionPreference = "Stop"

# Tauri keys and signatures are base64 of a minisign file; its second line is base64 of
# 2 algorithm bytes, the 8-byte key id, then the key or signature.
function Get-MinisignKeyId([string]$TauriBase64) {
    $text = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($TauriBase64.Trim()))
    $lines = @($text -split "`r?`n")
    if ($lines.Count -lt 2) { throw "not a minisign key or signature" }
    $bytes = [System.Convert]::FromBase64String($lines[1].Trim())
    if ($bytes.Length -lt 10) { throw "not a minisign key or signature" }
    [System.BitConverter]::ToString($bytes, 2, 8)
}

if ($PubKey -match 'PLACEHOLDER') { throw "the updater public key is still the placeholder" }
$expectedKeyId = Get-MinisignKeyId $PubKey

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
$sigText = (Get-Content $sig -Raw).Trim()
$decoded = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($sigText))
if ($decoded -notmatch ('version:\s*' + [regex]::Escape($Version))) {
    throw "the signature isn't bound to version $Version"
}
if ((Get-MinisignKeyId $sigText) -ne $expectedKeyId) {
    Remove-Item $sig -Force
    throw "the signing key doesn't match the updater public key in tauri.conf.json"
}
Write-Output "Signed: $sig"
