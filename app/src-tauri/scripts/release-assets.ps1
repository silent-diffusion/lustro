# Collects the release assets after build-installer.ps1 (used by .github/workflows/release.yml).
#
# Writes to -OutDir:
#   assets/Lustro-Setup-<v>.exe            the one-click installer
#   assets/Lustro-Setup-<v>.exe.sig        its updater signature (sign-installer.ps1)
#   assets/Lustro-<v>-portable-windows-x64.zip   lustro.exe (built with the `portable` feature: no
#                                          updater, no network) + the fast-scan helper + licenses
#   assets/latest.json                     the updater manifest the app checks
#   assets/SHA256SUMS.txt                  SHA-256 of every asset above
#   release-notes.md                       body of the GitHub Release
#
# The release notes (in latest.json, which the app shows, and on the release page) are the
# CHANGELOG.md section for <v>. Without a .sig (local test builds) latest.json is skipped, unless
# -RequireSigning, which makes that an error.
#
# Usage:  powershell -File app/src-tauri/scripts/release-assets.ps1 -Version 0.1.0 -OutDir release-out
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$OutDir,
    [string]$Repo = "silent-diffusion/lustro",
    [switch]$RequireSigning
)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $PSScriptRoot           # app/src-tauri
$root = Resolve-Path (Join-Path $here "..\..")     # workspace root
$utf8 = New-Object System.Text.UTF8Encoding $false # no BOM: the updater parses latest.json strictly

if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "version '$Version' is not x.y.z" }

# --- Release notes: the CHANGELOG.md section "## [<v>] ..." up to the next "## ".
$lines = Get-Content (Join-Path $root "CHANGELOG.md") -Encoding UTF8
$start = -1
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match ('^## \[' + [regex]::Escape($Version) + '\]')) { $start = $i; break }
}
if ($start -lt 0) { throw "CHANGELOG.md has no '## [$Version]' section" }
$section = @()
for ($i = $start + 1; $i -lt $lines.Count -and $lines[$i] -notmatch '^## '; $i++) { $section += $lines[$i] }
$notes = ($section -join "`n").Trim()
if (-not $notes) { throw "CHANGELOG.md section for $Version is empty" }

# The app shows its own notes (RELEASE_NOTES.md, built in) after updating; make sure they were
# updated for this version too.
$appNotes = Get-Content (Join-Path $here "RELEASE_NOTES.md") -Raw -Encoding UTF8
if ($appNotes -notmatch [regex]::Escape($Version)) { throw "app/src-tauri/RELEASE_NOTES.md doesn't mention $Version" }

# --- Assets
$assets = Join-Path $OutDir "assets"
if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Force $assets | Out-Null

$setupName = "Lustro-Setup-$Version.exe"
$nsis = Join-Path $root "target\release\bundle\nsis"
$setup = Join-Path $nsis $setupName
if (-not (Test-Path $setup)) { throw "$setupName not found; run build-installer.ps1 first" }
Copy-Item $setup $assets

$sigPath = "$setup.sig"
$hasSig = Test-Path $sigPath
if ($hasSig) {
    Copy-Item $sigPath $assets
} elseif ($RequireSigning) {
    throw "$setupName.sig not found; the build needs TAURI_SIGNING_PRIVATE_KEY"
}

# Portable zip: a separate build with the `portable` feature, which compiles out the updater, so
# the portable app makes no network requests and never turns itself into an installed copy. Its
# own target dir keeps it apart from the installer's exe. The helper sits next to the app exe
# (the app looks for it there and checks its hash), plus the licenses.
if (-not (Select-String -Path (Join-Path $here "Cargo.toml") -Pattern '^portable\s*=' -Quiet)) {
    throw "app/src-tauri/Cargo.toml has no 'portable' feature yet"
}
$tauri = Join-Path $here "..\ui\node_modules\.bin\tauri.cmd"
if (-not (Test-Path $tauri)) { throw "Tauri CLI not found: run 'npm ci' in app/ui first" }
foreach ($v in @("TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PATH", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD")) {
    if ([Environment]::GetEnvironmentVariable($v)) { throw "$v is set: builds must not see the signing key" }
}
$portableTarget = Join-Path $root "target\portable"
$prevTarget = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = $portableTarget
Push-Location $here
try {
    & $tauri build --no-bundle --no-sign --features portable --config tauri.portable.conf.json -- --no-default-features
    if ($LASTEXITCODE -ne 0) { throw "portable build failed" }
} finally {
    Pop-Location
    $env:CARGO_TARGET_DIR = $prevTarget
}
& (Join-Path $PSScriptRoot "check-helper-hash.ps1") -AppExe (Join-Path $portableTarget "release\lustro.exe")
$triple =((rustc -vV) | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
$stage = Join-Path $OutDir "portable"
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item (Join-Path $portableTarget "release\lustro.exe") $stage
Copy-Item (Join-Path $here "binaries\lustro-scan-helper-$triple.exe") (Join-Path $stage "lustro-scan-helper.exe")
Copy-Item (Join-Path $root "LICENSE") (Join-Path $stage "LICENSE.txt")
Copy-Item (Join-Path $root "THIRD-PARTY-NOTICES.md") (Join-Path $stage "THIRD-PARTY-NOTICES.md")
$zipName = "Lustro-$Version-portable-windows-x64.zip"
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath (Join-Path $assets $zipName)
Remove-Item $stage -Recurse -Force

# --- latest.json (Tauri updater manifest)
if ($hasSig) {
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
} else {
    Write-Warning "No updater signature: latest.json not written (local test build)."
}

# --- Checksums
$sums = Get-ChildItem $assets -File | Sort-Object Name | ForEach-Object {
    "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
}
[System.IO.File]::WriteAllText((Join-Path $assets "SHA256SUMS.txt"), (($sums -join "`n") + "`n"), $utf8)

# --- GitHub Release body
$body = @"
$notes

## Install on Windows

1. Download **$setupName** below and open it.
2. Windows may say **"Windows protected your PC"**, because the installer isn't code-signed yet.
   Click **More info**, then **Run anyway**.
3. Click **Install**. Lustro installs for your user only (no administrator password) and opens.

Already have Lustro? It updates itself; nothing to do.

**Other files:** ``$zipName`` is a portable version (unzip and run ``lustro.exe``). It never
updates itself and makes no network requests; download new versions from this page.
``SHA256SUMS.txt`` lists the SHA-256 of every file. ``latest.json`` and the ``.sig`` file are used by
the built-in updater.
"@
[System.IO.File]::WriteAllText((Join-Path $OutDir "release-notes.md"), $body.Replace("`r`n", "`n"), $utf8)

Get-ChildItem $assets | Format-Table Name, Length -AutoSize | Out-String | Write-Output
