#!/usr/bin/env bash
# Package Dioxus's Debian payload with GTK/WebKit runtime dependencies.
# An output ending in .AppDir stages the payload for Velopack instead.
set -euo pipefail

package="$(realpath "${1:?Usage: bundle-appimage.sh path/to/Tantrix.deb [output.AppImage]}")"
output="$(realpath -m "${2:-${package%.deb}.AppImage}")"
arch="$(uname -m)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
appdir="$work/Tantrix.AppDir"

# Keep the plugins and launcher together: their WebKit relocation expects cwd=usr.
tauri_revision=d53900fd3868a9dc13c1b7d655dc220dac48ac15
plugin_url="https://raw.githubusercontent.com/tauri-apps/tauri/$tauri_revision/crates/tauri-bundler/src/bundle/linux/appimage"
curl -fsSL "https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy-07333c6/linuxdeploy-$arch.AppImage" -o "$work/linuxdeploy.AppImage"
for plugin in gtk gstreamer; do
    curl -fsSL "$plugin_url/linuxdeploy-plugin-$plugin.sh" -o "$work/linuxdeploy-plugin-$plugin.sh"
done
mkdir -p "$appdir"
dpkg-deb -x "$package" "$appdir"
curl -fsSL "https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-$arch" -o "$appdir/AppRun"
chmod +x "$work/linuxdeploy.AppImage" "$work"/linuxdeploy-plugin-*.sh "$appdir/AppRun"

# WebKit's release build uses its compiled helper path. The GTK plugin relocates
# it relative to usr, so retain the builder's directory layout inside the AppDir.
libdir="$(pkg-config --variable=libdir webkit2gtk-4.1)"
mkdir -p "$appdir$libdir/webkit2gtk-4.1"
cp -a "$libdir/webkit2gtk-4.1/." "$appdir$libdir/webkit2gtk-4.1/"

# WebKit needs appsink even without media playback. Limit the standard plugin
# to these two modules instead of bundling every codec installed on the runner.
export GSTREAMER_PLUGINS_DIR="$work/gstreamer-plugins"
export GSTREAMER_HELPERS_DIR="$libdir/gstreamer1.0/gstreamer-1.0"
mkdir -p "$GSTREAMER_PLUGINS_DIR"
cp "$libdir/gstreamer-1.0/libgstapp.so" "$libdir/gstreamer-1.0/libgstcoreelements.so" "$GSTREAMER_PLUGINS_DIR/"

# linuxdeploy normally excludes these font libraries; include them explicitly.
libraries=()
for library in libharfbuzz.so.0 libfreetype.so.6 libfontconfig.so.1 libfribidi.so.0 libexpat.so.1; do
    libraries+=(--library "$libdir/$library")
done

export APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true ARCH="$arch"
export PATH="$work:$PATH"
mkdir -p "$(dirname "$output")"
cd "$work"
output_args=()
if [[ "$output" != *.AppDir ]]; then
    output_args=(--output appimage)
fi
OUTPUT="$work/Tantrix.AppImage" "$work/linuxdeploy.AppImage" \
    --appdir "$appdir" \
    --desktop-file "$appdir/usr/share/applications/tantrix.desktop" \
    --icon-file "$appdir/usr/share/icons/hicolor/256x256/apps/tantrix.png" \
    "${libraries[@]}" --plugin gtk --plugin gstreamer "${output_args[@]}"
if [[ "$output" == *.AppDir ]]; then
    mv "$appdir" "$output"
else
    mv "$work/Tantrix.AppImage" "$output"
fi
