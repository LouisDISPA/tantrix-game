"""Signing configuration and key cleanup without real distribution credentials."""

import importlib.util
import json
import os
from pathlib import Path
import tempfile
import subprocess
import traceback
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("pack_desktop", Path(__file__).with_name("pack-desktop.py"))
pack = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pack)


class SigningTests(unittest.TestCase):
    def setUp(self):
        self.environment = patch.dict(os.environ, {}, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.search_list = patch.object(pack.subprocess, "check_output",
                                       return_value='    "/Users/test/Library/Keychains/login.keychain-db"\n')
        self.search_list.start()
        self.addCleanup(self.search_list.stop)

    def test_unsigned_build_requires_no_credentials(self):
        self.assertEqual(pack.mac_signing(self.work), ([], None))
        with patch.object(pack, "run") as run:
            pack.pack_windows(self.work, self.work, "test", ["pack"])
            run.assert_called_once_with("vpk", "pack")
        with patch.object(pack, "run") as run:
            pack.sign_linux(self.work, "test", self.work)
            run.assert_not_called()

    def test_partial_configuration_fails_before_running_tools(self):
        for name, function in [
            ("MACOS_SIGN_IDENTITY", lambda: pack.mac_signing(self.work)),
            ("WINDOWS_CERTIFICATE_BASE64", lambda: pack.pack_windows(self.work, self.work, "test", ["pack"])),
            ("LINUX_GPG_FINGERPRINT", lambda: pack.sign_linux(self.work, "test", self.work)),
        ]:
            with self.subTest(name=name), patch.dict(os.environ, {name: "configured"}), \
                    patch.object(pack, "run") as run:
                with self.assertRaisesRegex(RuntimeError, "Incomplete signing setup"):
                    function()
                run.assert_not_called()

    def test_windows_password_stays_out_of_packaging_arguments(self):
        os.environ.update(WINDOWS_CERTIFICATE_BASE64="dGVzdA==", WINDOWS_CERTIFICATE_PASSWORD="secret")
        with patch.object(pack, "run") as run:
            pack.pack_windows(self.work, self.work, "test", ["pack", "--packDir", "path with spaces"])
        self.assertEqual((self.work / "certificate.pfx").read_bytes(), b"test")
        self.assertEqual(json.loads((self.work / "pack-arguments.json").read_text()),
                         ["pack", "--packDir", "path with spaces"])
        self.assertNotIn("secret", str(run.call_args))
        self.assertIn("-PublicCertificatePath", run.call_args.args)

    def test_mac_import_failure_deletes_temporary_keychain(self):
        os.environ.update(MACOS_CERTIFICATE_BASE64="dGVzdA==", MACOS_CERTIFICATE_PASSWORD="test",
                          MACOS_SIGN_IDENTITY="test")

        def fail_import(*args, **kwargs):
            if args[:2] == ("security", "import"):
                raise RuntimeError("import failed")

        with patch.object(pack, "run", side_effect=fail_import) as run:
            with self.assertRaisesRegex(RuntimeError, "import failed"):
                pack.mac_signing(self.work)
            self.assertEqual(run.call_args_list[-2].args,
                             ("security", "list-keychains", "-d", "user", "-s",
                              "/Users/test/Library/Keychains/login.keychain-db"))
            self.assertEqual(run.call_args.args[:2], ("security", "delete-keychain"))

    def test_mac_accepts_wrapped_base64_certificate(self):
        os.environ.update(MACOS_CERTIFICATE_BASE64="dGVz\ndA==\n", MACOS_CERTIFICATE_PASSWORD="test",
                          MACOS_SIGN_IDENTITY="test")
        with patch.object(pack, "run"):
            flags, keychain = pack.mac_signing(self.work)
        self.assertEqual((self.work / "certificate.p12").read_bytes(), b"test")
        self.assertNotIn("--notaryProfile", flags)
        self.assertIsNotNone(keychain)

    def test_mac_ci_trust_is_removed_even_when_keychain_cleanup_is_needed(self):
        os.environ.update(MACOS_CERTIFICATE_BASE64="dGVzdA==", MACOS_CERTIFICATE_PASSWORD="test",
                          MACOS_SIGN_IDENTITY="test", GITHUB_ACTIONS="true")
        with patch.object(pack, "run") as run:
            flags, keychain = pack.mac_signing(self.work)
            self.assertNotIn("--notaryProfile", flags)
            self.assertTrue((self.work / "trust-added").exists())
            pack.cleanup_mac(keychain)
        commands = [call.args for call in run.call_args_list]
        self.assertTrue(any(command[:4] == ("sudo", "-n", "security", "add-trusted-cert")
                            for command in commands))
        self.assertEqual(commands[-3][:4], ("sudo", "-n", "security", "remove-trusted-cert"))
        self.assertEqual(commands[-1][:2], ("security", "delete-keychain"))

    def test_mac_keychain_search_list_is_extended_and_restored(self):
        os.environ.update(MACOS_CERTIFICATE_BASE64="dGVzdA==", MACOS_CERTIFICATE_PASSWORD="test",
                          MACOS_SIGN_IDENTITY="test")
        original = ["/Users/Test User/Library/Keychains/login.keychain-db", "/tmp/other.keychain-db"]
        with patch.object(pack.subprocess, "check_output",
                          return_value="\n".join(f'    "{path}"' for path in original)), \
                patch.object(pack, "run") as run:
            _, keychain = pack.mac_signing(self.work)
            commands = [call.args for call in run.call_args_list]
            self.assertIn(("security", "list-keychains", "-d", "user", "-s", keychain, *original), commands)
            pack.cleanup_mac(keychain)
            self.assertEqual(run.call_args_list[-2].args,
                             ("security", "list-keychains", "-d", "user", "-s", *original))

    def test_mac_keychain_is_deleted_if_restoring_search_list_fails(self):
        (self.work / "keychain-search-list.json").write_text("[]")

        def fail_restore(*args, **kwargs):
            if args[:2] == ("security", "list-keychains"):
                raise RuntimeError("restore failed")

        with patch.object(pack, "run", side_effect=fail_restore) as run:
            with self.assertRaisesRegex(RuntimeError, "restore failed"):
                pack.cleanup_mac(self.work / "signing.keychain-db")
            self.assertEqual(run.call_args.args[:2], ("security", "delete-keychain"))

    def test_mac_keychain_is_deleted_if_removing_trust_fails(self):
        (self.work / "trust-added").touch()
        (self.work / "keychain-search-list.json").write_text('["/tmp/original.keychain-db"]')

        def fail_removal(*args, **kwargs):
            if args[:4] == ("sudo", "-n", "security", "remove-trusted-cert"):
                raise RuntimeError("trust removal failed")

        with patch.object(pack, "run", side_effect=fail_removal) as run:
            with self.assertRaisesRegex(RuntimeError, "trust removal failed"):
                pack.cleanup_mac(self.work / "signing.keychain-db")
            self.assertEqual(run.call_args_list[-2].args,
                             ("security", "list-keychains", "-d", "user", "-s", "/tmp/original.keychain-db"))
            self.assertEqual(run.call_args.args[:2], ("security", "delete-keychain"))

    def test_tool_failure_does_not_echo_secret_arguments(self):
        with patch.object(pack.subprocess, "run") as run:
            run.return_value.returncode = 1
            with self.assertRaises(RuntimeError) as failure:
                pack.run("signing-tool", "secret-password")
            self.assertNotIn("secret-password", str(failure.exception))

    def test_tool_timeout_does_not_echo_secret_arguments_in_traceback(self):
        password = "secret-password"
        error = subprocess.TimeoutExpired(["signing-tool", password], 180)
        with patch.object(pack.subprocess, "run", side_effect=error), \
                patch("builtins.print"):
            try:
                pack.run("signing-tool", password, label="Sign DMG", timeout=180)
            except RuntimeError:
                output = traceback.format_exc()
            else:
                self.fail("Expected signing timeout")
        self.assertIn("Sign DMG timed out after 180s", output)
        self.assertNotIn("secret-password", output)

    def test_cleanup_continues_after_trust_removal_timeout(self):
        (self.work / "trust-added").touch()
        (self.work / "keychain-search-list.json").write_text("[]")

        def timeout_removal(*args, **kwargs):
            if args[:4] == ("sudo", "-n", "security", "remove-trusted-cert"):
                raise RuntimeError("trust removal timed out")

        with patch.object(pack, "run", side_effect=timeout_removal) as run:
            with self.assertRaisesRegex(RuntimeError, "trust removal timed out"):
                pack.cleanup_mac(self.work / "signing.keychain-db")
        self.assertEqual(run.call_args.args[:2], ("security", "delete-keychain"))
        self.assertEqual(run.call_args.kwargs["timeout"], 60)

    def test_trust_cleanup_failure_is_nonfatal_only_on_disposable_runners(self):
        (self.work / "trust-added").touch()
        (self.work / "keychain-search-list.json").write_text("[]")

        def timeout_removal(*args, **kwargs):
            if args[:4] == ("sudo", "-n", "security", "remove-trusted-cert"):
                raise RuntimeError("trust removal timed out")

        for runner in ("github-hosted", "self-hosted"):
            with self.subTest(runner=runner), \
                    patch.dict(os.environ, GITHUB_ACTIONS="true", RUNNER_ENVIRONMENT=runner), \
                    patch.object(pack, "run", side_effect=timeout_removal) as run, \
                    patch("builtins.print") as output:
                if runner == "github-hosted":
                    pack.cleanup_mac(self.work / "signing.keychain-db")
                    self.assertIn("::warning::", output.call_args.args[0])
                else:
                    with self.assertRaisesRegex(RuntimeError, "trust removal timed out"):
                        pack.cleanup_mac(self.work / "signing.keychain-db")
                    output.assert_not_called()
                self.assertEqual(run.call_args_list[-2].args[:2], ("security", "list-keychains"))
                self.assertEqual(run.call_args.args[:2], ("security", "delete-keychain"))


if __name__ == "__main__":
    unittest.main()
