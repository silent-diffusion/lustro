# Finishes the release assets once the installer is signed: writes latest.json (the updater
# manifest) from the .sig and update-notes.md, and SHA256SUMS.txt over every asset. Needs only
# pwsh: in the release workflow it runs in the `release` job, which has no signing key and no
# source tree beyond this script.
#
# Expects in -Dir (the output of release-assets.ps1, plus the .sig from the `sign` job):
#   assets/Lustro-Setup-<v>.exe, assets/Lustro-Setup-<v>.exe.sig, update-notes.md
# -InstallerSha256: fail unless the installer is exactly the one that was built and signed.
# Without a .sig (local test builds) latest.json is skipped, unless -RequireSigning.
#
# Usage:  pwsh release-manifest.ps1 -Version 0.1.0 -Dir release-out [-Repo owner/name]
#         [-InstallerSha256 <hex>] [-RequireSigning]
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Dir,
    [string]$Repo = "silent-diffusion/lustro",
    [string]$InstallerSha256,
    [switch]$RequireSigning
)
$ErrorActionPreference = "Stop"
$utf8 = New-Object System.Text.UTF8Encoding $false # no BOM: the updater parses latest.json strictly

if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "version '$Version' is not x.y.z" }
$assets = Join-Path $Dir "assets"
$setupName = "Lustro-Setup-$Version.exe"
$setup = Join-Path $assets $setupName
if (-not (Test-Path $setup)) { throw "$setupName not found in $assets" }
if ($InstallerSha256) {
    $actual = (Get-FileHash $setup -Algorithm SHA256).Hash
    if ($actual -ne $InstallerSha256.ToUpperInvariant()) { throw "$setupName isn't the installer that was built and signed" }
}

$sigPath = "$setup.sig"
if (Test-Path $sigPath) {
    $notes = (Get-Content (Join-Path $Dir "update-notes.md") -Raw -Encoding UTF8).Trim()
    if (-not $notes) { throw "update-notes.md is empty" }
    $manifest = [ordered]@{
        version   = $Version
        notes     = $notes
        pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        platforms = [ordered]@{
            "windows-x86_64" = [ordered]@{
                signature = (Get-Content $sigPath -Raw).Trim()
                url       = "https://github.com/$Repo/releases/download/v$Version/$setupName"
            }
        }
    }
    [System.IO.File]::WriteAllText((Join-Path $assets "latest.json"), ($manifest | ConvertTo-Json -Depth 5), $utf8)
} elseif ($RequireSigning) {
    throw "$setupName.sig not found"
} else {
    Write-Warning "No updater signature: latest.json not written (local test build)."
}

$sums = Get-ChildItem $assets -File | Where-Object Name -ne "SHA256SUMS.txt" | Sort-Object Name | ForEach-Object {
    "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
}
[System.IO.File]::WriteAllText((Join-Path $assets "SHA256SUMS.txt"), (($sums -join "`n") + "`n"), $utf8)

Get-ChildItem $assets | Format-Table Name, Length -AutoSize | Out-String | Write-Output
