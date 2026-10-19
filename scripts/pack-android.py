#!/usr/bin/env python3
"""Build and sign the Android GitHub release using stock Dioxus alpha.3."""
import base64
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib

def replace(path, old, new):
    source = path.read_text()
    if new in source:
        return
    if old not in source:
        raise RuntimeError(f"Dioxus template changed: {path}")
    path.write_text(source.replace(old, new, 1))


def prepare(project, root):
    replace(project / "build.gradle.kts", "gradle:8.7.0", "gradle:8.10.1")
    replace(project / "gradle/wrapper/gradle-wrapper.properties",
            "gradle-9.1.0-bin.zip", "gradle-8.11.1-bin.zip")
    app = project / "app/build.gradle.kts"
    replace(app, "versionCode = 1", 'versionCode = System.getenv("ANDROID_VERSION_CODE").toInt()')
    replace(app, '    buildTypes {', '''    signingConfigs {
        create("release") {
            storeFile = file(System.getenv("ANDROID_KEYSTORE_PATH"))
            storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("ANDROID_KEY_ALIAS")
            keyPassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
        }
    }
    buildTypes {''')
    replace(app, '        getByName("release") {', '''        getByName("release") {
            signingConfig = signingConfigs.getByName("release")''')
    resources = project / "app/src/main/res"
    for source in [root / "tantrix-app/icons/android", root / "tantrix-app/android/res"]:
        shutil.copytree(source, resources, dirs_exist_ok=True)


def run(*args, **kwargs):
    try:
        return subprocess.run(args, check=True, **kwargs)
    except subprocess.CalledProcessError as error:
        for output in (error.stdout, error.stderr):
            if output:
                print(output.decode(errors="replace") if isinstance(output, bytes) else output,
                      file=sys.stderr)
        raise


def main():
    root = Path(__file__).resolve().parent.parent
    if sys.argv[1:] == ["--prepare"]:
        prepare(Path.cwd(), root)
        return
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise SystemExit("Android packaging runs in the Release workflow.")
    version = tomllib.loads((root / "tantrix-app/Cargo.toml").read_text())["package"]["version"]
    if os.environ["GITHUB_REF_TYPE"] == "tag" and os.environ["GITHUB_REF_NAME"] != f"tantrix-app-v{version}":
        raise SystemExit("Release tag must match the app version")
    major, minor, patch = map(int, version.split("-")[0].split("."))
    code = major * 1_000_000 + minor * 1_000 + patch
    if not (minor < 1000 and patch < 1000 and 0 < code <= 2100000000):
        raise SystemExit("Version exceeds the Android version code scheme")
    java_home = os.environ["JAVA_HOME"]
    with tempfile.TemporaryDirectory(dir=os.environ["RUNNER_TEMP"]) as temp:
        work = Path(temp)
        key = work / "tantrix.p12"
        key.write_bytes(base64.b64decode(os.environ["ANDROID_KEYSTORE_BASE64"], validate=True))
        key.chmod(0o600)
        shim = work / "bin/java"
        shim.parent.mkdir()
        shim.write_text(JAVA_LAUNCHER)
        shim.chmod(0o755)
        env = {**os.environ, "TANTRIX_REPO_ROOT": str(root), "TANTRIX_JAVA_HOME": java_home,
               "JAVA_HOME": str(work), "ANDROID_KEYSTORE_PATH": str(key),
               "ANDROID_VERSION_CODE": str(code)}
        run("dx", "build", "--locked", "--release", "--platform", "android", "--package", "tantrix-app",
            "--target", "aarch64-linux-android", "--features", "android", cwd=root, env=env)
        apk = root / "target/dx/tantrix/release/android/app/app/build/outputs/apk/release/app-release.apk"
        tools = Path(os.environ["ANDROID_HOME"]) / "build-tools/35.0.0"
        result = run(str(tools / "apksigner"), "verify", "--print-certs", str(apk), capture_output=True, text=True)
        certificate = run(str(Path(java_home) / "bin/keytool"), "-exportcert", "-keystore", str(key),
                          "-storepass:env", "ANDROID_KEYSTORE_PASSWORD", "-alias", os.environ["ANDROID_KEY_ALIAS"],
                          capture_output=True).stdout
        expected = hashlib.sha256(certificate).hexdigest()
        if f"Signer #1 certificate SHA-256 digest: {expected}" not in result.stdout:
            raise SystemExit("APK signing certificate mismatch")
        run(str(tools / "zipalign"), "-c", "-P", "16", "4", str(apk))
        artifacts = root / "artifacts"
        artifacts.mkdir(exist_ok=True)
        output = artifacts / f"Tantrix-{version}-android-aarch64.apk"
        shutil.copyfile(apk, output)
        output.with_suffix(".apk.sha256").write_text(f"{hashlib.sha256(output.read_bytes()).hexdigest()}  {output.name}\n")
        print(f"Signed APK: {output.name}")


JAVA_LAUNCHER = r"""#!/usr/bin/env bash
# dx alpha.3 generates its Gradle project just before calling this JVM.
set -euo pipefail
args=("$@")
for arg in "$@"; do
  if [[ "$arg" == org.gradle.wrapper.GradleWrapperMain || "$arg" == */gradle-wrapper.jar ]]; then
    python3 "$TANTRIX_REPO_ROOT/scripts/pack-android.py" --prepare
    # Without inline signing credentials dx chooses assembleDebug, even for --release.
    for i in "${!args[@]}"; do
      [[ "${args[$i]}" != assembleDebug ]] || args[$i]=assembleRelease
    done
    break
  fi
done
export JAVA_HOME="$TANTRIX_JAVA_HOME"
exec "$JAVA_HOME/bin/java" "${args[@]}"
"""

if __name__ == "__main__":
    main()
