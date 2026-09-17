#!/bin/sh
# Knowlith installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/PetarVukovic/knowlith/main/install.sh | sh
#
# Written in POSIX sh rather than bash, because macOS ships bash 3.2 and the
# one thing an installer may never do is fail on the machine it was written
# for. No sudo: everything lands in the user's own home, which is also what
# makes it removable by deleting two directories.
#
# Every step announces itself. An installer that prints nothing and then
# fails is indistinguishable from one that hung, and the person watching has
# no way to tell which.

set -eu

REPO="${KNOWLITH_REPO:-PetarVukovic/knowlith}"
VERSION="${KNOWLITH_VERSION:-latest}"
BIN_DIR="${KNOWLITH_BIN_DIR:-$HOME/.local/bin}"

say() { printf '  %s\n' "$*"; }
die() { printf '\nknowlith: %s\n' "$*" >&2; exit 1; }

printf '\nKnowlith\n'

# ---------------------------------------------------------------- platform --

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin) os_tag="apple-darwin" ;;
  Linux)  os_tag="unknown-linux-gnu" ;;
  *) die "$os is not supported. macOS, Linux and Windows are." ;;
esac

case "$arch" in
  arm64|aarch64) arch_tag="aarch64" ;;
  x86_64|amd64)  arch_tag="x86_64" ;;
  *) die "$arch is not supported." ;;
esac

target="${arch_tag}-${os_tag}"
say "$os $arch"

# An Intel binary running under Rosetta on Apple silicon works, but it is
# half the speed for no reason, so it is worth saying out loud.
if [ "$os" = "Darwin" ] && [ "$arch_tag" = "x86_64" ]; then
  if [ "$(sysctl -in sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
    say "this shell is running under Rosetta — installing the Intel build"
  fi
fi

for tool in curl tar; do
  command -v "$tool" >/dev/null 2>&1 || die "$tool is needed and is not installed."
done

# ----------------------------------------------------------------- version --

if [ "$VERSION" = "latest" ]; then
  say "asking GitHub for the latest release"
  VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' \
    | head -n 1)"
  [ -n "$VERSION" ] || die "could not work out the latest version. Set KNOWLITH_VERSION and try again."
fi
say "version $VERSION"

archive="knowlith-${VERSION}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/${VERSION}/${archive}"

# ---------------------------------------------------------------- download --

work="$(mktemp -d)"
# Runs on success, failure and Ctrl-C alike, so a failed install leaves
# nothing behind in the temp directory.
trap 'rm -rf "$work"' EXIT INT TERM

say "downloading"
curl -fsSL "$url" -o "$work/$archive" \
  || die "could not download $url
  If that version has no build for $target, the release page lists what there is."

# The checksum is published beside the archive and the install fails closed:
# no checksums file, no entry for this archive, no sha256 tool — each of those
# stops here rather than installing a binary nobody has vouched for. This
# used to be optional "for older releases"; there are no older releases, and
# an installer that verifies only when convenient verifies nothing. The
# owner can still say so explicitly with KNOWLITH_ALLOW_UNVERIFIED=1.
verify() {
  curl -fsSL "https://github.com/${REPO}/releases/download/${VERSION}/checksums.txt" -o "$work/checksums.txt" 2>/dev/null \
    || return 1
  expected="$(grep " ${archive}\$" "$work/checksums.txt" | awk '{print $1}' || true)"
  [ -n "$expected" ] || return 2
  if command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$work/$archive" | awk '{print $1}')"
  elif command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$work/$archive" | awk '{print $1}')"
  else
    return 3
  fi
  [ "$actual" = "$expected" ] || die "the download does not match its published checksum. Nothing was installed."
  say "checksum matches"
}

if verify; then
  :
else
  # `$?` here is verify's own status; behind a `!` it would be the negation's.
  case "$?" in
    1) reason="this release publishes no checksums.txt" ;;
    2) reason="checksums.txt has no entry for $archive" ;;
    *) reason="neither shasum nor sha256sum is installed here" ;;
  esac
  if [ "${KNOWLITH_ALLOW_UNVERIFIED:-}" = "1" ]; then
    say "installing unverified because KNOWLITH_ALLOW_UNVERIFIED=1 ($reason)"
  else
    die "the download could not be verified: $reason.
  Nothing was installed. To install anyway: KNOWLITH_ALLOW_UNVERIFIED=1 sh install.sh"
  fi
fi

tar -xzf "$work/$archive" -C "$work" || die "the archive could not be opened."
[ -f "$work/knowlith" ] || die "the archive did not contain a knowlith binary."

# ----------------------------------------------------------------- install --

mkdir -p "$BIN_DIR"
# Written beside the target and moved, so an upgrade cannot leave a
# half-written binary where a working one used to be — and so replacing a
# running binary does not fail with "text file busy".
mv "$work/knowlith" "$BIN_DIR/knowlith.new"
chmod +x "$BIN_DIR/knowlith.new"
mv -f "$BIN_DIR/knowlith.new" "$BIN_DIR/knowlith"
say "installed to $BIN_DIR/knowlith"

# macOS quarantines anything downloaded, and the first run is then refused
# by Gatekeeper with a dialog that does not explain itself.
if [ "$os" = "Darwin" ] && command -v xattr >/dev/null 2>&1; then
  xattr -d com.apple.quarantine "$BIN_DIR/knowlith" 2>/dev/null || true
fi

# -------------------------------------------------------------------- path --

case ":$PATH:" in
  *":$BIN_DIR:"*) on_path=1 ;;
  *) on_path=0 ;;
esac

if [ "$on_path" = "0" ]; then
  # Appended to whichever shell profile exists, rather than to a guessed
  # one. A line in a file the shell does not read is worse than no line,
  # because it looks like it should have worked.
  line="export PATH=\"$BIN_DIR:\$PATH\""
  added=""
  for profile in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile"; do
    if [ -f "$profile" ] && ! grep -Fq "$BIN_DIR" "$profile" 2>/dev/null; then
      printf '\n# Knowlith\n%s\n' "$line" >> "$profile"
      added="$added $profile"
    fi
  done
  if [ -n "$added" ]; then
    say "added $BIN_DIR to your PATH in:$added"
    say "open a new terminal, or run: $line"
  else
    say "add this to your shell profile: $line"
  fi
fi

# ------------------------------------------------------------------ set up --

printf '\n'
"$BIN_DIR/knowlith" --version 2>/dev/null || die "the binary was installed but will not run."

printf '\nNext:\n'
printf '  %s scan ~/Documents/YourCompany   read a folder\n' "knowlith"
printf '  %s start                          open the interface in your browser\n' "knowlith"
printf '  %s serve                          same, without opening a browser tab\n' "knowlith"
printf '  %s connect                        hand it to Claude and Codex\n' "knowlith"
printf '  %s autostart on                   keep it running when you close the window\n' "knowlith"
printf '\nWhat it reads is kept in ~/Knowlith. Knowlith itself sends nothing anywhere;\n'
printf 'the AI tool you choose to read with (Claude, Codex, Cursor) sends document text to its vendor.\n\n'
