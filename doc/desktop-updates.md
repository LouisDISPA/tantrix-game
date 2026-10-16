# Desktop updates

Velopack 1.2.161 checks GitHub Releases once per launch and downloads updates in the background. Navigation and online compatibility reuse that result. Menus show progress, then **Restart and update**; installation requires a click. Games never show restart controls. Legacy, `.deb` and development installs fall back to a download link. The game server address does not affect the update source.

## Packages and release channels

Each OS/architecture has a separate `stable-<os>-<arch>` channel and package ID; prereleases use `preview-…`. Stable installs ignore prereleases. Only newer versions install, after checking package hashes against the GitHub-hosted feed.

`scripts/pack-desktop.py` packages the Dioxus output with `vpk`, then stages:

| Platform | Download | How updates install |
| --- | --- | --- |
| macOS | DMG with the Velopack-enabled `.app` | Replace the whole app bundle and relaunch |
| Windows | Per-user installer or portable ZIP | Velopack's helper replaces the application and relaunches |
| Linux | AppImage | Replace the AppImage and relaunch |
| Debian/Ubuntu | `.deb` | Download/install a newer package manually |

The Linux AppImage retains the GTK/WebKit/GStreamer payload assembled by
`scripts/bundle-appimage.sh`. Passing a `.AppDir` output stages it for Velopack
instead of generating a standalone AppImage.

Publish `releases.<channel>.json` with every `.nupkg` it references. Never rename update packages. Windows portable downloads must include the entire ZIP. Existing unmanaged releases need a one-time managed install; old NSIS installations can then be removed separately.

## Local macOS build

Install the .NET 8 SDK and `dotnet tool install -g vpk --version 1.2.161`, then:

```sh
dx bundle --locked --release --platform macos --package tantrix-app --features desktop,bundle --package-types macos --out-dir bundles
python3 scripts/pack-desktop.py macos aarch64 0.1.3
```

Use the app version from `tantrix-app/Cargo.toml` and the build's architecture.
Outputs are in `artifacts/`. Copy the app out of the DMG before running it.

## Free self-signed releases

macOS and Windows use self-signed code-signing certificates; Linux uses GPG. Generate keys **once**, back them up outside Git, reuse across versions/architectures and renew before expiry. No paid signing service is required.

Self-signing verifies who holds the key; it does **not** establish public publisher
trust. Windows SmartScreen warnings can remain. macOS builds are **not notarized**
and Gatekeeper can require an explicit user approval. Linux signatures are checked
manually; the updater still uses Velopack feed hashes, not GPG verification.

The workflow signs the app and its updater/installer before update feeds are
created. It verifies signatures before uploading. Public certificates/keys are
included in release downloads; private keys stay in temporary build storage.

### 1. macOS: create a certificate once

1. Open **Keychain Access → Certificate Assistant → Create a Certificate**.
2. Name it **Tantrix releases**, choose **Self Signed Root** and **Code Signing**.
   Enable the option to override defaults if you want a longer validity, e.g.
   730 days. Save it to the login keychain.
3. In **My Certificates**, export the certificate **and its private key** as a
   password-protected `.p12`. Save it outside this repository.
4. Encode it to your clipboard:

   ```sh
   base64 -i /path/to/tantrix.p12 | pbcopy
   ```

5. In **GitHub → Settings → Secrets and variables → Actions**, add repository
   **secrets**:

   | Secret | Value |
   | --- | --- |
   | `MACOS_CERTIFICATE_BASE64` | Clipboard contents from step 4 |
   | `MACOS_CERTIFICATE_PASSWORD` | Password protecting the `.p12` |
   | `MACOS_SIGN_IDENTITY` | `Tantrix releases` (the certificate's exact name) |

GitHub's macOS runner temporarily trusts the certificate for code signing and
imports its key into a temporary keychain. Velopack signs the app and updater;
the packaging script signs the DMG. Both signatures are verified without requiring
notarization. Build trust and the temporary keychain are removed afterward.

For a local signed build, first open the certificate in Keychain Access, expand
**Trust**, and set **Code Signing → Always Trust**. Confirm the change yourself.
The script only manages temporary trust automatically on GitHub Actions runners.

### 2. Windows: create a certificate once

Open PowerShell on Windows. Use a private directory outside this repository and run:

```powershell
$certificate = New-SelfSignedCertificate -Type Custom `
    -Subject "CN=Tantrix releases" -FriendlyName "Tantrix releases" `
    -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 `
    -KeyUsage DigitalSignature -KeyExportPolicy Exportable `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}") `
    -CertStoreLocation Cert:\CurrentUser\My `
    -NotAfter (Get-Date).AddYears(2)
$password = Read-Host "Choose a PFX password" -AsSecureString
Export-PfxCertificate -Cert $certificate -FilePath .\tantrix.pfx -Password $password
Export-Certificate -Cert $certificate -FilePath .\tantrix.cer
[Convert]::ToBase64String([IO.File]::ReadAllBytes((Resolve-Path .\tantrix.pfx))) | Set-Clipboard
```

The extensions explicitly set Code Signing and Basic Constraints (end entity,
`CA=FALSE`, no path-length constraint). A CA certificate cannot sign the app under
this setup. If an existing certificate fails with `0x80096019`, create a replacement
and update both Windows secrets; importing it into another trust store does not
repair its signed extensions.

Add repository **secrets**:

| Secret | Value |
| --- | --- |
| `WINDOWS_CERTIFICATE_BASE64` | Base64 PFX from the clipboard |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password chosen above |

The build imports the PFX into a dedicated temporary certificate store. Velopack
signs and timestamps the app, updater and installer. The script temporarily trusts
the public certificate in the build machine's Local Machine root store (no UI
prompt) to verify the signatures, then removes
the imported private key, trust and temporary store. Passwords are not passed to
Velopack or included in command arguments. Local signed Windows builds require an
elevated PowerShell; GitHub-hosted runners already have the required access.

### 3. Linux: create a GPG key once

Install GnuPG, then run:

```sh
gpg --quick-generate-key "Tantrix releases" ed25519 sign 2y
gpg --list-secret-keys --keyid-format long
gpg --armor --export-secret-keys FINGERPRINT
```

Replace `FINGERPRINT` with the full fingerprint shown. Add repository **secrets**
`LINUX_GPG_PRIVATE_KEY` (the complete armored export) and `LINUX_GPG_PASSPHRASE`
(the nonempty key password). Set repository **variable** `LINUX_GPG_FINGERPRINT`
to the full fingerprint.

Each Linux build publishes its public key, `SHA256SUMS-<channel>.txt`, and a `.asc`
signature covering the final downloads, feed and update packages. The build
verifies the signature using a temporary keyring.

### 4. Test and release

Run **Actions → Release → Run workflow** on the branch containing these changes.
It builds downloadable artifacts without publishing a release. Install them and
test an older-to-newer update on each OS before creating a release tag normally.

No GitHub environment is required. With a platform's signing settings absent, it builds unsigned; partial settings or failed signatures stop publication. Publish public-key SHA-256 fingerprints separately from downloads so players can compare them independently.

### What players need to do

- **macOS:** copy the app from the DMG into Applications. After a blocked launch,
  use **System Settings → Privacy & Security → Open Anyway**, if offered. Trusting
  our certificate alone does not replace notarization or guarantee Gatekeeper
  approval. Do not disable Gatekeeper globally.
- **Windows:** installation warnings can remain. Players who want to trust the
  publisher can verify the downloaded public certificate fingerprint, then import
  that **public `.cer` only** into **Current User → Trusted Root Certification
  Authorities**. This does not guarantee SmartScreen approval; system policy may
  still block execution. Never distribute or import the private `.pfx` to players.
- **Linux:** verify the public-key fingerprint, signature and file checksums:

  ```sh
  gpg --import signing-key-stable-linux-x86_64.asc
  gpg --fingerprint
  gpg --verify SHA256SUMS-stable-linux-x86_64.txt.asc SHA256SUMS-stable-linux-x86_64.txt
  sha256sum --ignore-missing --check SHA256SUMS-stable-linux-x86_64.txt
  ```

macOS publishes `signing-key-<channel>.pem`; show its SHA-256 fingerprint with
`openssl x509 -in signing-key-<channel>.pem -noout -fingerprint -sha256`.
Windows publishes `signing-key-<channel>.cer`; inspect it in Certificate Manager
or use `certutil -hashfile signing-key-<channel>.cer SHA256`.

References: [Apple self-signed certificates](https://support.apple.com/en-ca/guide/keychain-access/kyca8916/mac),
[opening a blocked Mac app](https://support.apple.com/en-us/102445),
[Windows certificate creation](https://learn.microsoft.com/en-us/powershell/module/pki/new-selfsignedcertificate),
[Windows SmartScreen behavior](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

## Verification

Signing configuration and temporary keychain cleanup tests:

```sh
python3 -B -m unittest discover -s scripts -p 'test_pack_desktop.py'
```

Desktop unit tests cover no-update and corrupt-download paths:

```sh
cargo test -p tantrix-app --features desktop --lib
```

Verify download, hash validation and offline recovery of a locally packaged update:

```sh
TANTRIX_UPDATE_FEED="$PWD/artifacts/releases.stable-macos-aarch64.json" cargo test -p tantrix-app --features desktop packaged_update_is_verified_and_remains_ready_offline --lib -- --ignored
```

This test uses a temporary installation and never replaces the running app.
Before publishing, exercise an older-to-newer update with actual installed
packages on each OS and architecture, including restart, offline retry and
locations requiring elevated permissions.

References: [Velopack Rust integration](https://docs.velopack.io/getting-started/rust),
[packaging](https://docs.velopack.io/packaging/overview),
[signing](https://docs.velopack.io/packaging/signing).
