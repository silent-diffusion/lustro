# Security policy

Lustro reads your whole drive and can move files to the Recycle Bin, so we take security reports
seriously. Thank you for helping keep its users safe.

## Reporting a vulnerability

**Please don't open a public issue for security problems.** Report them privately instead:

1. Go to the repository's [**Security** tab](https://github.com/silent-diffusion/lustro/security).
2. Click **Report a vulnerability** (GitHub private vulnerability reporting).
3. Describe the problem, the version you tested, and the steps to reproduce it. A proof of concept
   helps, but please use test files and folders, never someone else's data.

We aim to acknowledge a report within 7 days and to agree on a fix and a disclosure date with you.
We'll credit you in the release notes unless you prefer not to be named.

## Supported versions

Only the latest release gets security fixes. Lustro updates itself, so most users are on it within
a day of a release.

## In scope

- **The update path:** anything that lets an update install without a valid signature, a
  downgrade, or a manifest or package that isn't from this project's GitHub Releases.
- **Cleanup safety:** anything that deletes permanently instead of using the Recycle Bin, removes
  a protected path, or acts on a file the user didn't approve.
- **Privilege:** the elevated fast-scan helper doing anything beyond reading the volume index for
  the app that launched it, or being launchable or replaceable by another program.
- **The app's web view:** script injection, loading remote content, or calling commands the UI
  isn't meant to have (CSP and Tauri capabilities bypasses).
- **IPC:** the UI getting the app to act on a path the user didn't scan or pick.
- **Privacy:** any network request other than the update check, or any leak of file paths or scan
  data off the computer.
- **Scanning:** anything that makes a scan write to, open for writing or download (hydrate) a file.
- **The build and release pipeline** in `.github/workflows`, including secret exposure.

## Known limitations

- **Release notes aren't signed.** The update manifest (`latest.json`) carries the version's
  release notes next to the installer's signature, but the signature covers only the installer.
  Someone able to change `latest.json` on our GitHub Releases could change the notes text, though
  not what gets installed. To limit this, the release workflow builds the notes only from
  `CHANGELOG.md` in the tagged commit, the signing key is only available to a protected `release`
  environment, and the app shows the notes as plain text (no HTML, no clickable links, no images).
- **The installer isn't code-signed yet**, so Windows SmartScreen warns on first install. Updates
  are still verified with the updater signature.

## Out of scope

- Attacks that already need administrator rights or full control of the user's account.
- Windows SmartScreen warnings about the installer: it isn't code-signed yet, and this is known.
- Denial of service by scanning an extremely large or deliberately malformed folder tree, unless it
  corrupts data or escapes the app.
- Vulnerabilities in third-party dependencies with no impact on Lustro; please report them
  upstream. (Dependabot and `cargo deny` / `npm audit` already run in CI.)
