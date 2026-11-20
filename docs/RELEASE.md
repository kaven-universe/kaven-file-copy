# Release Process

Kaven File Copy uses separate development and public release histories:

- `origin/main` on Gitea retains the complete development history.
- `github/release` contains one parentless snapshot commit for the current release.
- GitHub Releases host the signed Windows installer, updater signature, candidate manifest, CLI, desktop executable, and checksums.
- `public.data.kaven.xyz` is the only update-discovery endpoint used by installed applications.

The GitHub repository is public so updater assets can be downloaded without credentials. Its default branch is `release`; each release replaces that branch with a new parentless snapshot using `--force-with-lease`. Version tags retain published release snapshots.

## Protected signing configuration

The GitHub `release` Environment must define `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The private key must match the public key in `apps/desktop/src-tauri/tauri.conf.json`. Never commit or upload the private key, password, or DPAPI cache.

## Release procedure

1. Prepare and commit the numeric version, reviewed notes under `docs/releases/`, and all source changes on Gitea `main`.
2. Run `scripts/test-release-identity.ps1 -Tag X.Y.Z`, `npm run verify`, Rust formatting, Clippy, tests, and both dependency audits.
3. Build a signed local candidate and confirm the installer, `.sig`, executable version, and hashes.
4. Create a parentless commit from the final `HEAD` tree:

   ```powershell
   $tree = (git rev-parse 'HEAD^{tree}').Trim()
   $snapshot = (git commit-tree $tree -m "Kaven File Copy X.Y.Z release snapshot").Trim()
   ```

5. Read the current GitHub `release` branch and replace it safely:

   ```powershell
   $line = git ls-remote --heads github refs/heads/release
   $previous = ($line -split '\s+')[0]
   git push --force-with-lease="refs/heads/release:$previous" github "${snapshot}:refs/heads/release"
   ```

6. Create an annotated tag object pointing to the snapshot and push it to GitHub. Never move or reuse a published tag.
7. Verify the GitHub Actions run and published Release. Every updater URL in `latest.json` must work without authentication, and the downloaded installer hash must match the signed candidate.
8. Copy the verified candidate manifest into `json/kaven-file-copy/X.Y.Z.json`, prepend the entry in `versions.json`, and replace `latest.json` last in the public-data repository.
9. Commit and push public-data to Gitea, publish its current committed snapshot to GitHub Pages, and poll the live canonical URL until it announces the new release.

The canonical endpoint is:

```text
https://public.data.kaven.xyz/json/kaven-file-copy/latest.json
```

GitHub's `latest.json` Release asset is evidence only. Installed applications never discover updates through GitHub directly; the public-data manifest may point to the immutable GitHub Release installer.

## Recovery

If a published version is defective, do not replace its tag, installer, signature, or version-specific manifest. Withdraw it where applicable and publish a higher patch version. If the GitHub snapshot branch changed unexpectedly, stop rather than replacing `--force-with-lease` with an unconditional force push.
