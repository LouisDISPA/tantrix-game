# Android releases

Push `tantrix-app-v<version>` to publish the signed ARM64 APK and its `.sha256`
with desktop releases. **Actions → Release → Run workflow** builds artifacts
without publishing. Packaging runs only in GitHub Actions.

- Android 14+; package `io.github.louisdispa.tantrix`.
- CI uses stock Dioxus 0.8.0-alpha.3. `scripts/pack-android.py` adjusts its generated
  Gradle project, signs with the required release key, and verifies the APK.
- GitHub secrets: `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`.
  The key's password must be the keystore password.
- Key backup: Proton Pass → **Tantrix signing** → **Tantrix - Release signing**.
  Keep it: updates require the same certificate.
- Version code: `major × 1,000,000 + minor × 1,000 + patch`, ignoring any `-` suffix,
  so a prerelease and its release share a code. Increase for each release.
- The app checks the latest stable GitHub release at startup. **Download and install**
  downloads privately, checks the package and signing certificate, then opens Android’s
  installer. Allow installs when prompted, then tap **Install update**.
  Velopack supports desktop only.

[Android signing](https://developer.android.com/studio/publish/app-signing) ·
[Velopack](https://docs.velopack.io/)
