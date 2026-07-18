# Building & Distributing Better iMessage

This guide covers building the app locally and shipping a signed, notarized macOS
`.dmg`. It is macOS-only.

- [Prerequisites](#prerequisites)
- [1. Release build](#1-release-build)
- [2. Running the unsigned build locally](#2-running-the-unsigned-build-locally)
- [3. Distributing to others (sign → notarize → staple)](#3-distributing-to-others-sign--notarize--staple)
- [Permissions & why the Mac App Store is impossible](#permissions--why-the-mac-app-store-is-impossible)
- [Quick reference](#quick-reference)

---

## Prerequisites

- **macOS 12+** and **Xcode Command Line Tools** (`xcode-select --install`).
  Notarization tooling (`xcrun notarytool`, `xcrun stapler`) ships with the CLT.
- **Rust** (stable) via <https://rustup.rs>.
- **Node.js 18+** and npm.
- `npm install` once, to get the Tauri CLI and frontend dependencies.
- **For a distributable build only:** a paid **Apple Developer Program**
  membership and a **Developer ID Application** certificate in your login
  keychain. (Not needed to build or run locally.)

The first `--features fastembed` build **downloads the ONNX Runtime binary** (via
the `ort` crate's `download-binaries`) and release-compiles the workspace, so it
is significantly slower and needs network access. Subsequent builds are cached.

---

## 1. Release build

The real, on-device semantic-search model (`BAAI/bge-small-en-v1.5`) is behind
the `fastembed` Cargo feature, so that dev builds stay model-free and offline. The
release build turns it on:

```bash
npm run build:release
# which is exactly:
npm run tauri build -- --features fastembed
```

The `fastembed` feature flows `better-im-app → better-im-index/fastembed →
fastembed → ort` (see `src-tauri/Cargo.toml` and `index/Cargo.toml`). Without it,
the app builds and runs using a deterministic mock embedder — fine for
development, but not the shipping semantic model.

> The `bge-small` model weights themselves are downloaded on **first use at
> runtime** (when the user first builds the semantic index), not at build time.

### Where the artifacts land

Relative to the repo root, under `src-tauri/target/release/bundle/`:

| Target | Path |
| ------ | ---- |
| `.app` | `macos/Better iMessage.app` |
| `.dmg` | `dmg/Better iMessage_0.1.0_aarch64.dmg` (arch varies) |

The bundle metadata (category, copyright, descriptions, `LSMinimumSystemVersion`
= 12.0, DMG layout, hardened-runtime entitlements) comes from the `bundle` block
in `src-tauri/tauri.conf.json` and `src-tauri/entitlements.plist`.

---

## 2. Running the unsigned build locally

An unsigned `.app`/`.dmg` runs on **your own Mac** without a Developer account.
macOS Gatekeeper will quarantine it, so use the **right-click → Open** path once:

1. Open the `.dmg` and drag **Better iMessage** to `/Applications`.
2. In `/Applications`, **right-click (or Control-click) the app → Open**.
3. Confirm **Open** in the "unidentified developer" dialog. macOS remembers the
   choice; subsequent launches are normal double-clicks.

If it was downloaded/transferred and Gatekeeper still refuses, clear quarantine:

```bash
xattr -dr com.apple.quarantine "/Applications/Better iMessage.app"
```

Then grant **Full Disk Access** (and optionally **Contacts**) when prompted — see
below.

---

## 3. Distributing to others (sign → notarize → staple)

To hand the `.dmg` to anyone else, it must be **signed with a Developer ID
Application certificate**, **notarized** by Apple, and **stapled**. Replace every
`NAME`, `TEAMID`, and credential placeholder with your own.

You can let Tauri do the signing during the build, or sign manually. Both are
shown.

### 3a. Sign

**Option A — let Tauri sign during the build** (recommended). Set these before
`npm run build:release`:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: NAME (TEAMID)"
# optional, if your login keychain is locked in CI:
# export APPLE_CERTIFICATE / APPLE_CERTIFICATE_PASSWORD / APPLE_KEYCHAIN_PASSWORD
npm run build:release
```

Tauri applies the **hardened runtime** (`bundle.macOS.hardenedRuntime: true`) and
the entitlements in `src-tauri/entitlements.plist` automatically.

**Option B — sign the built `.app` manually:**

```bash
codesign --deep --force \
  --options runtime \
  --entitlements src-tauri/entitlements.plist \
  --sign "Developer ID Application: NAME (TEAMID)" \
  "src-tauri/target/release/bundle/macos/Better iMessage.app"

# verify
codesign --verify --strict --verbose=2 "…/Better iMessage.app"
spctl -a -vvv --type execute "…/Better iMessage.app"   # (will say "rejected" until notarized+stapled)
```

`--options runtime` enables the hardened runtime (required for notarization).

### 3b. Notarize

Notarize the **`.dmg`** (or a zipped `.app`). Two credential styles work — pick one.

**Using an Apple ID + app-specific password:**

```bash
xcrun notarytool submit \
  "src-tauri/target/release/bundle/dmg/Better iMessage_0.1.0_aarch64.dmg" \
  --apple-id "you@example.com" \
  --team-id "TEAMID" \
  --password "APP_SPECIFIC_PASSWORD" \
  --wait
```

> Generate the app-specific password at <https://appleid.apple.com> → Sign-In &
> Security → App-Specific Passwords. It is **not** your Apple ID password.

**Or using an App Store Connect API key** (better for CI):

```bash
xcrun notarytool submit "…/Better iMessage_0.1.0_aarch64.dmg" \
  --key   "AuthKey_KEYID.p8" \
  --key-id "KEYID" \
  --issuer "ISSUER_UUID" \
  --wait
```

You can store either as a named profile once and reuse it:

```bash
xcrun notarytool store-credentials "better-im-notary" \
  --apple-id "you@example.com" --team-id "TEAMID" --password "APP_SPECIFIC_PASSWORD"
xcrun notarytool submit "…/….dmg" --keychain-profile "better-im-notary" --wait
```

If notarization fails, read the log:

```bash
xcrun notarytool log <submission-id> --keychain-profile "better-im-notary"
```

### 3c. Staple

Once notarization **Accepted**, staple the ticket so the app validates offline:

```bash
xcrun stapler staple "src-tauri/target/release/bundle/dmg/Better iMessage_0.1.0_aarch64.dmg"
# (optional) also staple the .app inside, if distributing the .app directly:
xcrun stapler staple "src-tauri/target/release/bundle/macos/Better iMessage.app"

# final Gatekeeper check — should now pass:
spctl -a -vvv --type install "…/….dmg"
```

The stapled `.dmg` is ready to distribute.

---

## Permissions & why the Mac App Store is impossible

- **Full Disk Access is NOT an entitlement.** It is a user-granted **TCC**
  permission that the person installing the app must turn on themselves in
  **System Settings › Privacy & Security › Full Disk Access**. You cannot request
  or bundle it; you can only prompt the user to grant it (the app's onboarding
  screen does exactly this). The same is true of **Contacts** access, which uses
  the `NSContactsUsageDescription` string in `src-tauri/Info.plist`.
- **The app is intentionally NOT sandboxed** (`com.apple.security.app-sandbox` =
  `false` in `entitlements.plist`). Reading another app's data under
  `~/Library/Messages` is impossible inside the App Sandbox.
- **Therefore Mac App Store distribution is impossible.** The MAS mandates the App
  Sandbox, which forbids reading `chat.db`. Better iMessage can only be
  distributed **outside** the store, via a Developer ID–signed, notarized `.dmg`
  (the flow above). This is a fundamental constraint of what the app does, not a
  packaging choice.
- **Hardened runtime + third-party native code.** The release
  (`--features fastembed`) build loads the ONNX Runtime native library, which is
  not signed by your Team ID. `entitlements.plist` grants
  `com.apple.security.cs.disable-library-validation` so the hardened runtime
  permits it. The default keyword-only build loads no such library.

---

## Quick reference

```bash
# Dev (model-free, offline)
npm run tauri dev

# Release build with the real semantic model
npm run build:release            # tauri build --features fastembed

# Sign (manual)
codesign --deep --options runtime --entitlements src-tauri/entitlements.plist \
  --sign "Developer ID Application: NAME (TEAMID)" "…/Better iMessage.app"

# Notarize + staple
xcrun notarytool submit "…/….dmg" --apple-id "you@example.com" \
  --team-id "TEAMID" --password "APP_SPECIFIC_PASSWORD" --wait
xcrun stapler staple "…/….dmg"
```

Artifacts: `src-tauri/target/release/bundle/{macos,dmg}/`.
