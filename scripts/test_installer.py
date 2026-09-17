"""Installer checks use local archives, never GitHub or the owner's install."""
import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"

class InstallerTests(unittest.TestCase):
    def install(self, binary, checksum=True):
        with tempfile.TemporaryDirectory(prefix="knowlith-install-test-") as directory:
            root = Path(directory)
            mock = root / "mock"
            target = root / "bin"
            mock.mkdir()
            target.mkdir()
            installed = target / "knowlith"
            previous = b"#!/bin/sh\necho old-working-install\n"
            installed.write_bytes(previous)
            installed.chmod(0o755)
            archive = root / "archive.tar.gz"
            with tarfile.open(archive, "w:gz") as tar:
                info = tarfile.TarInfo("knowlith")
                info.size, info.mode = len(binary), 0o755
                tar.addfile(info, io.BytesIO(binary))
            curl = mock / "curl"
            curl.write_text("""#!/bin/sh
url=""
out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    https:*) url="$1"; shift ;;
    *) shift ;;
  esac
done
case "$url" in
  */checksums.txt) [ "$FIXTURE_CHECKSUM" = 1 ] || exit 22; printf '%s  %s\\n' "$FIXTURE_HASH" "$FIXTURE_ARCHIVE" > "$out" ;;
  *.tar.gz) cp "$FIXTURE_DIR/archive.tar.gz" "$out" ;;
  *) exit 22 ;;
esac
""")
            curl.chmod(0o755)
            system = os.uname()
            arch = "aarch64" if system.machine in ("arm64", "aarch64") else "x86_64"
            platform = "apple-darwin" if system.sysname == "Darwin" else "unknown-linux-gnu"
            env = {**os.environ, "PATH": f"{mock}:{target}:{os.environ['PATH']}", "KNOWLITH_BIN_DIR": str(target),
                   "KNOWLITH_NO_START": "1", "KNOWLITH_VERSION": "v-test", "FIXTURE_DIR": str(root),
                   "FIXTURE_CHECKSUM": "1" if checksum else "0", "FIXTURE_HASH": hashlib.sha256(archive.read_bytes()).hexdigest(),
                   "FIXTURE_ARCHIVE": f"knowlith-v-test-{arch}-{platform}.tar.gz"}
            result = subprocess.run(["sh", str(INSTALLER)], env=env, capture_output=True, timeout=15)
            return result.returncode, installed.read_bytes(), previous, result.stdout + result.stderr

    def test_a_broken_download_does_not_replace_the_working_install(self):
        code, actual, previous, output = self.install(b"#!/bin/sh\nexit 1\n")
        self.assertNotEqual(code, 0, output)
        self.assertEqual(actual, previous)

    def test_missing_checksums_leave_the_working_install_untouched(self):
        code, actual, previous, output = self.install(b"#!/bin/sh\necho knowlith-test\n", checksum=False)
        self.assertNotEqual(code, 0, output)
        self.assertEqual(actual, previous)

    def test_a_verified_working_binary_is_installed(self):
        binary = b"#!/bin/sh\necho knowlith-test\n"
        code, actual, _, output = self.install(binary)
        self.assertEqual(code, 0, output)
        self.assertEqual(actual, binary)

if __name__ == "__main__":
    unittest.main()
