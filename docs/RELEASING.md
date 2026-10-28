# Releasing Lustro

Releases are built by GitHub Actions (`.github/workflows/release.yml`), never on a personal machine.

## One-time setup: the updater key pair

The updater key is separate from Windows code signing. The **private key never goes in the repo**;
it lives only in the GitHub Actions secrets and in an offline backup kept by a maintainer.

Do these steps in order. The protected environment must exist **before** any secret is added, so
the key is never available to an unprotected workflow run.

1. **Two-factor authentication:** make sure every account with write access to the repository
   has it turned on.
2. **Create the protected environment.** Repository **Settings → Environments → New environment**,
   name it `release`, then **Configure environment**:
   1. Tick **Required reviewers** and add at least one maintainer. Optionally tick **Prevent
      self-review**.
   2. Under **Deployment branches and tags**, choose **Selected branches and tags**, then
      **Add deployment branch or tag rule** → type **Tag**, pattern `v*`. Remove any other rule.
   3. Click **Save protection rules**.
3. **Generate the key pair** on a trusted machine, outside any git checkout (after `npm ci` in
   `app/ui`): `app/ui/node_modules/.bin/tauri signer generate -w <a path outside the repo>`.
   Choose a strong password.
4. **Add the secrets to the environment, not the repository.** In the same `release` environment
   page, under **Environment secrets → Add environment secret**, add:
   - `TAURI_SIGNING_PRIVATE_KEY`: the private key file's contents;
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password.

   Don't add them under **Secrets and variables → Actions → Repository secrets**.
5. Put the **public** key (`.pub` file contents) in `app/src-tauri/tauri.conf.json` →
   `plugins.updater.pubkey`, replacing the placeholder, and commit that.
6. Back up the private key and password offline. If the key is lost, installed copies can't be
   updated any more; if it leaks, revoke it by shipping a new public key in a release signed with
   the old one, and rotate the secrets.

## Each release

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`).
2. In `CHANGELOG.md`, move the **Unreleased** items into a new `## [x.y.z] - YYYY-MM-DD` section,
   written for users. The app shows this text when it offers the update.
3. Update `app/src-tauri/RELEASE_NOTES.md` (the notes the app shows after updating) for x.y.z.
4. Merge to `main` with CI green, then tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.
5. A required reviewer approves the `release` deployment in the workflow run.
6. The workflow checks the tag matches the version and that the public key is set, then:
   - builds `Lustro-Setup-x.y.z.exe` **without** the key (`tauri build --no-sign`), so no
     dependency's build script or npm tool ever sees it;
   - signs it in a separate step that runs only the pinned Tauri CLI
     (`tauri signer sign --app-version x.y.z`), bound to the version;
   - builds the portable zip, then `latest.json` from the `.sig`, and `SHA256SUMS.txt`;
   - checks both app exes pin the freshly built fast-scan helper;
   - creates a **draft** release.
7. Review the draft: download the installer, check it installs and runs, compare the checksum.
   Then **Publish**. From that moment `releases/latest/download/latest.json` points at it and
   installed copies update within a day.
