# Desktop release process

The `Release` workflow builds desktop packages in one native runner matrix, then creates the GitHub Release only after every platform succeeds. GitHub generates the release notes; version suffixes such as `-rc.1` produce prereleases.

## Release a version

1. Update `tantrix-app/Cargo.toml`'s package version and commit the release.
2. Push a tag matching the app package's version, for example:

   ```sh
   git tag tantrix-app-v0.1.1
   git push origin tantrix-app-v0.1.1
   ```

3. Wait for the `Release` workflow. It produces macOS DMGs for Apple Silicon and Intel, Linux `.deb` and AppImage packages for x86_64 and ARM64, and Windows portable and NSIS setup executables for x86_64 and ARM64.

The desktop bundles are built on native GitHub-hosted runners. `dx bundle` does not cross-build native desktop packages; Linux packages also rely on WebKitGTK being available on the build runner. The Windows NSIS bundle uses its per-user install mode and the small WebView2 download bootstrapper. NSIS uses its standard Modern UI wizard.

## Why these formats

- **macOS:** DMG is a familiar drag-to-Applications download. Release both supported desktop architectures separately; DMGs are currently unsigned.
- **Linux:** AppImage is a convenient portable download; `.deb` integrates with Debian/Ubuntu package managers and declares GTK/WebKitGTK runtime dependencies.
- **Windows:** Dioxus/tauri-bundler's NSIS installer provides a small guided per-user install and WebView2 bootstrapper. The portable option is the app executable itself. MSIX is the Microsoft package format to consider later for Store/managed deployment, but it is not the simplest output supported by Dioxus's current bundler.

## Local packaging commands

Run each command on its target OS after installing the Dioxus desktop prerequisites:

```sh
dx bundle --release --macos --package tantrix-app --features desktop,bundle --package-types macos --package-types dmg
dx bundle --release --linux --package tantrix-app --features desktop,bundle --package-types deb --out-dir bundles
bash scripts/bundle-appimage.sh bundles/*.deb
dx bundle --release --windows --package tantrix-app --features desktop,bundle --package-types nsis
```

On Linux, Dioxus builds the `.deb`; `scripts/bundle-appimage.sh` extracts its executable, resources and metadata into an AppDir and packages the AppImage once. Pinned Tauri GTK/GStreamer plugins handle modules, schemas, WebKit path relocation and runtime hooks. The script stages matching WebKit helpers, includes the excluded font libraries, and limits GStreamer to its appsink/core plugins. It requires the desktop development packages plus `curl`, `pkg-config`, `librsvg2-dev`, `patchelf`, `dpkg-dev` and `gstreamer1.0-plugins-base`. The release workflow runs this step automatically. It does not run a graphical startup check.

The AppImage still relies on the host's graphics stack (X11, OpenGL/EGL/OpenGL ES, GBM and DRM), fonts, CA certificates and a compatible glibc. The Ubuntu 24.04 build runners set that compatibility baseline; the published 0.1.0 executable requires glibc 2.39. Build on an older Linux baseline to support older distributions. Hosts without FUSE can use `APPIMAGE_EXTRACT_AND_RUN=1 ./Tantrix.AppImage`.
