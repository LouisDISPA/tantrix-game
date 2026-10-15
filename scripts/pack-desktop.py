#!/usr/bin/env python3
"""Wrap Dioxus's payload in Velopack, then stage downloads and update feeds."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import shlex
import shutil
import subprocess
import tempfile
import time


def run(*args, label=None, timeout=None):
    started = time.monotonic()
    if label:
        print(f"Starting: {label} (timeout: {timeout}s)", flush=True)
    try:
        result = subprocess.run([str(arg) for arg in args], timeout=timeout)
    except subprocess.TimeoutExpired:
        # Suppress the original exception, whose command may contain secrets.
        raise RuntimeError(f"{label or args[0]} timed out after {timeout}s") from None
    if result.returncode:
        # Signing commands contain credentials; do not echo their arguments
        # through CalledProcessError's traceback.
        raise RuntimeError(f"{args[0]} failed (exit {result.returncode})")
    if label:
        print(f"Complete: {label} ({time.monotonic() - started:.1f}s)", flush=True)


def single(folder, pattern):
    matches = list(folder.glob(pattern))
    if len(matches) != 1:
        raise RuntimeError(f"Expected one {pattern} in {folder}, found {matches}")
    return matches[0]


def configured(*names):
    """Allow unsigned builds, but never silently accept partial signing setup."""
    if not any(os.environ.get(name) for name in names):
        return False
    missing = [name for name in names if not os.environ.get(name)]
    if missing:
        raise RuntimeError("Incomplete signing setup; missing: " + ", ".join(missing))
    return True


def mac_signing(work):
    """Optional CI signing; local/test packages work without credentials."""
    if not configured("MACOS_CERTIFICATE_BASE64", "MACOS_CERTIFICATE_PASSWORD", "MACOS_SIGN_IDENTITY"):
        return [], None
    certificate = os.environ["MACOS_CERTIFICATE_BASE64"]
    identity = os.environ["MACOS_SIGN_IDENTITY"]
    keychain = work / "signing.keychain-db"
    password = secrets.token_hex(24)
    p12 = work / "certificate.p12"
    p12.write_bytes(base64.b64decode("".join(certificate.split()), validate=True))
    p12.chmod(0o600)
    search_list = shlex.split(subprocess.check_output(
        ["security", "list-keychains", "-d", "user"], text=True))
    (work / "keychain-search-list.json").write_text(json.dumps(search_list))
    run("security", "create-keychain", "-p", password, keychain)
    try:
        # --keychain restricts identity lookup, but codesign still needs this
        # keychain on the user's search list to resolve the certificate chain.
        run("security", "list-keychains", "-d", "user", "-s", keychain, *search_list)
        run("security", "unlock-keychain", "-p", password, keychain)
        run("security", "set-keychain-settings", "-lut", "21600", keychain)
        run("security", "import", p12, "-P", os.environ["MACOS_CERTIFICATE_PASSWORD"],
            "-k", keychain, "-T", "/usr/bin/codesign")
        run("security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
            "-s", "-k", password, keychain)
        public = work / "certificate.pem"
        run("security", "export", "-k", keychain, "-t", "certs", "-f", "pemseq", "-o", public)
        if os.environ.get("GITHUB_ACTIONS") == "true":
            # Hosted runners are disposable and have passwordless sudo. Local
            # signing requires trusting this identity manually in Keychain Access.
            run("sudo", "-n", "security", "add-trusted-cert", "-d", "-r", "trustRoot",
                "-p", "codeSign", "-k", keychain, public)
            (work / "trust-added").touch()
    except Exception:
        cleanup_mac(keychain)
        raise
    return ["--signAppIdentity", identity, "--keychain", keychain], keychain


def cleanup_mac(keychain):
    work = keychain.parent
    try:
        if (work / "trust-added").exists():
            try:
                run("sudo", "-n", "security", "remove-trusted-cert", "-d", work / "certificate.pem",
                    label="Remove temporary certificate trust", timeout=60)
            except RuntimeError:
                if (os.environ.get("GITHUB_ACTIONS") != "true"
                        or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"):
                    raise
                # The hosted VM is discarded after this job. A stalled trust
                # service must not reject a successfully signed package.
                print("::warning::Could not remove temporary certificate trust; "
                      "the GitHub-hosted runner will be discarded after this job.", flush=True)
    finally:
        try:
            saved_list = work / "keychain-search-list.json"
            if saved_list.exists():
                run("security", "list-keychains", "-d", "user", "-s",
                    *json.loads(saved_list.read_text()), label="Restore keychain search list", timeout=60)
        finally:
            run("security", "delete-keychain", keychain, label="Delete temporary keychain", timeout=60)


def pack_windows(work, artifacts, channel, arguments):
    if not configured("WINDOWS_CERTIFICATE_BASE64", "WINDOWS_CERTIFICATE_PASSWORD"):
        run("vpk", *arguments)
        return
    certificate = work / "certificate.pfx"
    certificate.write_bytes(base64.b64decode(
        "".join(os.environ["WINDOWS_CERTIFICATE_BASE64"].split()), validate=True))
    args_file = work / "pack-arguments.json"
    args_file.write_text(json.dumps([str(arg) for arg in arguments]))
    # The password is read from the environment by PowerShell, never CLI arguments.
    run("pwsh", "-NoProfile", "-NonInteractive", "-File", "scripts/pack-windows.ps1",
        "-CertificatePath", certificate, "-ArgumentsPath", args_file,
        "-PublicCertificatePath", artifacts / f"signing-key-{channel}.cer")


def sign_linux(artifacts, channel, work):
    if not configured("LINUX_GPG_PRIVATE_KEY", "LINUX_GPG_PASSPHRASE", "LINUX_GPG_FINGERPRINT"):
        return
    home = work / "gnupg"
    home.mkdir(mode=0o700)
    gpg = ["gpg", "--homedir", home, "--batch", "--yes"]
    key = work / "signing-key.asc"
    key.write_text(os.environ["LINUX_GPG_PRIVATE_KEY"])
    key.chmod(0o600)
    phrase = work / "passphrase"
    phrase.write_text(os.environ["LINUX_GPG_PASSPHRASE"])
    phrase.chmod(0o600)
    try:
        run(*gpg, "--import", key)
        # These files are staged only after packaging, so hashes cover final bytes.
        checksums = artifacts / f"SHA256SUMS-{channel}.txt"
        files = sorted(path for path in artifacts.iterdir()
                       if path.is_file() and not path.name.startswith(("SHA256SUMS-", "signing-key-")))
        checksums.write_text("".join(
            f"{sha256(path)}  {path.name}\n"
            for path in files
        ))
        public_key = artifacts / f"signing-key-{channel}.asc"
        run(*gpg, "--armor", "--output", public_key, "--export", os.environ["LINUX_GPG_FINGERPRINT"])
        run(*gpg, "--pinentry-mode", "loopback", "--passphrase-file", phrase,
            "--local-user", os.environ["LINUX_GPG_FINGERPRINT"], "--armor",
            "--detach-sign", checksums)
        run(*gpg, "--verify", f"{checksums}.asc", checksums)
    finally:
        run("gpgconf", "--homedir", home, "--kill", "gpg-agent")


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def package(platform, arch, version):
    root = Path(__file__).resolve().parent.parent
    os.chdir(root)
    bundles = root / "bundles"
    artifacts = root / "artifacts"
    artifacts.mkdir(exist_ok=True)
    lane = "preview" if "-" in version else "stable"
    channel = f"{lane}-{platform}-{arch}"
    cpu = {"x86_64": "x64", "aarch64": "arm64"}[arch]
    system = {"macos": "osx", "windows": "win", "linux": "linux"}[platform]
    prefix = f"Tantrix-{version}-{platform}-{arch}"

    if platform == "macos":
        payload = single(bundles, "*.app")
        icon = root / "tantrix-app/icons/icon.icns"
        extra = ["--noInst"]  # Keep DMG distribution; no second pkg installer.
    elif platform == "linux":
        payload = None  # Stage a fresh AppDir in the temporary directory below.
        icon = root / "tantrix-app/icons/256x256.png"
        extra = ["--categories", "Game;BoardGame"]
    else:
        payload = root / "target/dx/tantrix/release/windows/app"
        icon = root / "tantrix-app/icons/icon.ico"
        extra = ["--framework", "webview2"]

    with tempfile.TemporaryDirectory(prefix="tantrix-package-") as temp:
        work = Path(temp)
        output = work / "releases"
        keychain = None
        try:
            if platform == "linux":
                payload = work / "Tantrix.AppDir"
                run("bash", "scripts/bundle-appimage.sh", single(bundles, "*.deb"), payload)
            if platform == "macos":
                signing, keychain = mac_signing(work)
                extra += signing
            # Velopack's pending-package cache is keyed by app ID. Separate it
            # by channel and CPU so an Intel/preview download cannot be reused
            # by a different installation.
            arguments = ["pack", "--packId", f"Tantrix-{channel}", "--packTitle", "Tantrix",
                "--packAuthors", "Louis DISPA", "--packVersion", version,
                "--packDir", payload, "--mainExe", "tantrix.exe" if platform == "windows" else "tantrix",
                "--icon", icon, "--runtime", f"{system}-{cpu}", "--channel", channel,
                "--delta", "None", "--outputDir", output, "--skip-updates", "-y", *extra]
            if platform == "windows":
                pack_windows(work, artifacts, channel, arguments)
            else:
                run("vpk", *arguments)

            # These filenames are referenced by the feed: never rename them.
            feed = output / f"releases.{channel}.json"
            for asset in json.loads(feed.read_text())["Assets"]:
                shutil.copy2(output / asset["FileName"], artifacts)
            shutil.copy2(feed, artifacts)

            if platform == "macos":
                image = work / "dmg"
                image.mkdir()
                run("ditto", "-x", "-k", single(output, "*-Portable.zip"), image)
                if keychain:
                    run("codesign", "--verify", "--deep", "--strict", "--verbose=4", single(image, "*.app"),
                        label="Verify signed application", timeout=120)
                    shutil.copy2(work / "certificate.pem", artifacts / f"signing-key-{channel}.pem")
                (image / "Applications").symlink_to("/Applications")
                dmg = artifacts / f"{prefix}.dmg"
                run("hdiutil", "create", "-volname", "Tantrix", "-srcfolder", image,
                    "-ov", "-format", "UDZO", dmg, label="Create DMG", timeout=300)
                if keychain:
                    run("codesign", "--force", "--verbose=4", "--timestamp", "--sign",
                        os.environ["MACOS_SIGN_IDENTITY"], "--keychain", keychain, dmg,
                        label="Sign DMG with secure timestamp", timeout=180)
                    run("codesign", "--verify", "--strict", "--verbose=4", dmg,
                        label="Verify signed DMG", timeout=120)
            elif platform == "linux":
                shutil.copy2(single(output, "*.AppImage"), artifacts / f"{prefix}.AppImage")
                shutil.copy2(single(bundles, "*.deb"), artifacts / f"{prefix}.deb")
                sign_linux(artifacts, channel, work)
            else:
                shutil.copy2(single(output, "*-Setup.exe"), artifacts / f"{prefix}-setup.exe")
                shutil.copy2(single(output, "*-Portable.zip"), artifacts / f"{prefix}-portable.zip")
        finally:
            if keychain:
                cleanup_mac(keychain)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=["macos", "linux", "windows"])
    parser.add_argument("arch", choices=["aarch64", "x86_64"])
    parser.add_argument("version")
    args = parser.parse_args()
    package(args.platform, args.arch, args.version)
