# Releasing Agentry

A release is a version tag. Pushing `v0.2.0` makes GitHub Actions build a
universal (Apple Silicon + Intel) `Agentry.app`, sign and notarise it (and
the DMG, stapled, so it opens without a Gatekeeper check), and
create a **draft** GitHub Release with `Agentry.dmg`, `SHA256SUMS` and the
auto-update files. Publishing the draft updates the Homebrew cask.

## Every release

1. Set the version in `app/src-tauri/tauri.conf.json` and `app/package.json`
   (and `version` in the root `Cargo.toml`). The release fails if the tag and
   `tauri.conf.json` disagree.
2. Commit, then tag and push:

   ```sh
   git tag v0.2.0 && git push origin v0.2.0
   ```

3. Wait for the **Release** workflow, check the draft on GitHub, and publish it.
4. The **Homebrew** workflow then updates the cask in your tap.

## One-time setup

All of these are optional: without them the workflow still builds, but the app
is unsigned (macOS warns people opening it), doesn't auto-update, and Homebrew
isn't updated. Add them under the repository's **Settings → Secrets and
variables → Actions**.

### Signing and notarisation (Apple Developer account)

1. In Xcode or at developer.apple.com, create a **Developer ID Application**
   certificate. Export it from Keychain Access as a `.p12` with a password.
2. Add these **secrets**:

   | Secret | Value |
   |---|---|
   | `APPLE_CERTIFICATE` | `base64 -i certificate.p12 \| pbcopy`, then paste |
   | `APPLE_CERTIFICATE_PASSWORD` | the `.p12` export password |
   | `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Loops Tech (ABCDE12345)` (`security find-identity -v -p codesigning`) |
   | `APPLE_ID` | your Apple ID email |
   | `APPLE_PASSWORD` | an [app-specific password](https://account.apple.com) for that Apple ID |
   | `APPLE_TEAM_ID` | your 10-character team id |

### Auto-update

1. Generate a signing key pair (keep the private key safe; losing it means
   existing installs can't verify future updates):

   ```sh
   cd app && pnpm tauri signer generate -w ~/.tauri/agentry.key
   ```

2. Add the **secrets** `TAURI_SIGNING_PRIVATE_KEY` (the contents of
   `~/.tauri/agentry.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Add the **variable** (not secret) `UPDATER_PUBKEY`: the contents of
   `~/.tauri/agentry.key.pub`.

Installed copies check
`https://github.com/<repo>/releases/latest/download/latest.json` at launch
and daily, and install updates if "Install updates automatically" is on.

### Homebrew

1. Create a public repository named `homebrew-tap` (e.g. `loopstech/homebrew-tap`).
   Then `brew install --cask loopstech/tap/agentry` works.
2. Create a fine-grained token with **Contents: read and write** on that
   repository only, and add it as the secret `HOMEBREW_TAP_TOKEN`.
3. If the tap has a different name, set the variable `HOMEBREW_TAP`
   (e.g. `myorg/homebrew-tap`).

## Building locally

```sh
scripts/build-sidecars.sh                    # this machine (or `universal`)
cd app && pnpm tauri build --config src-tauri/tauri.release.json
```

The app is in `target/release/bundle/macos/`. The bundled `agentry-hook` and
`agentry` sit next to the app binary in `Agentry.app/Contents/MacOS/`.
