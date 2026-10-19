# Desktop releases

`.github/workflows/release.yml` builds six native OS/architecture combinations, then publishes a GitHub Release only after every build succeeds. Desktop jobs use stable Rust, Dioxus CLI 0.8.0-alpha.3 and Velopack 1.2.161.

## Publish

1. Update `tantrix-app/Cargo.toml`'s version and commit.
2. Push a matching tag, e.g. `tantrix-app-v0.1.3`.
3. Wait for **Release**. A version containing `-` creates a prerelease; GitHub generates notes.

Use **Actions → Release → Run workflow** to build downloadable artifacts without publishing. Optional macOS-only and Windows-only inputs build x86_64 signing diagnostics.

| OS | Architectures | Downloads |
| --- | --- | --- |
| macOS | aarch64, x86_64 | DMG |
| Linux | aarch64, x86_64 | AppImage, `.deb` |
| Windows | aarch64, x86_64 | Setup EXE, portable ZIP |

`scripts/pack-desktop.py` wraps Dioxus payloads with Velopack and writes downloads, feeds and update packages to `artifacts/`. Windows ZIPs include the updater; distributing a standalone EXE loses it. See [updates and signing](desktop-updates.md) for keys, feeds and installation tests.

## Local packaging

Run on the target OS with desktop prerequisites, .NET 8 and `vpk` 1.2.161 installed:

```sh
dotnet tool install -g vpk --version 1.2.161
# macOS
dx bundle --locked --release --platform macos --package tantrix-app --features desktop,bundle --package-types macos --out-dir bundles
# Linux
dx bundle --locked --release --platform linux --package tantrix-app --features desktop,bundle --package-types deb --out-dir bundles
# Windows
dx build --locked --release --platform windows --package tantrix-app --features desktop,bundle
# Package the matching payload; substitute OS, architecture and app version.
python3 scripts/pack-desktop.py macos aarch64 0.1.3
```

Dioxus does not cross-build native desktop bundles. Linux requires GTK/WebKit development packages plus `curl`, `pkg-config`, `librsvg2-dev`, `patchelf`, `dpkg-dev` and `gstreamer1.0-plugins-base`; the workflow lists its installed packages.

The Linux packager extracts the `.deb` into an AppDir through `scripts/bundle-appimage.sh`, then Velopack creates the AppImage. The AppDir includes WebKit helpers, font libraries and minimal GStreamer plugins. Packaging does not verify graphical startup.

AppImages still require host graphics libraries, fonts, CA certificates and compatible glibc. Ubuntu 24.04 runners set the build baseline; use older runners to support older distributions. Without FUSE, run `APPIMAGE_EXTRACT_AND_RUN=1 ./Tantrix.AppImage`.
