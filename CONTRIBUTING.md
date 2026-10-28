# Contributing to Lustro

Thanks for your interest! Bug reports, ideas and pull requests are all welcome.

## Before you start

- For anything bigger than a small fix, please open an issue first so we can agree on the approach.
- Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## Set up

See **Build from source** in the [README](README.md). Then install the secret-scanning hook:

```bash
pip install pre-commit
pre-commit install
```

It runs [gitleaks](https://github.com/gitleaks/gitleaks) with `.gitleaks.toml` on every commit.

## Rules that keep Lustro safe and private

1. **Fake data only.** Tests, fixtures, screenshots and examples use made-up paths and names
   (for example `C:\Users\William`, `/home/william`). Never commit a real path, username, email,
   key or scan result. Tests that touch the disk use temporary folders.
2. **No new network access.** The update check is the only network request the app makes. No
   telemetry, analytics or remote assets (fonts and libraries are bundled).
3. **Scanning is read-only** and never downloads cloud files. The only destructive operation is
   "move to Recycle Bin", behind the review queue.
4. **Keep dependencies few and well known.** New dependencies must pass `cargo deny check`
   (licenses and advisories) and `npm audit`.

## Before you open a pull request

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo test --workspace
cd app/ui && npm run check && npm test
```

CI runs the same checks on Windows, macOS and Linux, plus gitleaks, `cargo deny` and `npm audit`.
Add a line to the **Unreleased** section of [CHANGELOG.md](CHANGELOG.md) for anything users will
notice, written for users rather than developers.

By contributing you agree that your contribution is licensed under the [MIT License](LICENSE).
