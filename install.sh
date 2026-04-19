#!/usr/bin/env sh
set -eu

REPO="oneqit/qmux"
BIN_NAME="qmux"

VERSION=""
BIN_DIR=""
VERIFY_CHECKSUM=1
QUIET=0

usage() {
  cat <<'EOF'
Install qmux prebuilt binaries from GitHub Releases.

Usage:
  install.sh [options]

Options:
  --version <vX.Y.Z>  Install a specific release tag (default: latest)
  --bin-dir <path>    Install directory (default: /usr/local/bin if writable, else ~/.local/bin)
  --no-verify         Skip checksum verification
  -q, --quiet         Reduce output
  -h, --help          Show this help
EOF
}

log() {
  if [ "$QUIET" -ne 1 ]; then
    printf '%s\n' "$*"
  fi
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

normalize_version() {
  case "$1" in
    v*) printf '%s' "$1" ;;
    *) printf 'v%s' "$1" ;;
  esac
}

detect_asset() {
  os=$(uname -s 2>/dev/null || true)
  arch=$(uname -m 2>/dev/null || true)

  case "$os" in
    Darwin)
      case "$arch" in
        arm64|aarch64) printf 'qmux-aarch64-apple-darwin.tar.gz' ;;
        *) die "unsupported macOS architecture: $arch (supported: arm64)" ;;
      esac
      ;;
    Linux)
      case "$arch" in
        x86_64|amd64) printf 'qmux-x86_64-unknown-linux-gnu.tar.gz' ;;
        *) die "unsupported Linux architecture: $arch (supported: x86_64)" ;;
      esac
      ;;
    *)
      die "unsupported OS: $os (supported: macOS, Linux)"
      ;;
  esac
}

select_bin_dir() {
  if [ -n "$BIN_DIR" ]; then
    printf '%s' "$BIN_DIR"
    return
  fi

  if [ -w "/usr/local/bin" ]; then
    printf '%s' "/usr/local/bin"
    return
  fi

  [ -n "${HOME:-}" ] || die "HOME is not set; pass --bin-dir explicitly"
  printf '%s' "$HOME/.local/bin"
}

sha256_file() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$file" | awk '{print $NF}'
  else
    die "no SHA-256 tool found (need sha256sum, shasum, or openssl)"
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || die "--version requires a value"
      VERSION=$(normalize_version "$2")
      shift 2
      ;;
    --bin-dir)
      [ "$#" -ge 2 ] || die "--bin-dir requires a value"
      BIN_DIR="$2"
      shift 2
      ;;
    --no-verify)
      VERIFY_CHECKSUM=0
      shift
      ;;
    -q|--quiet)
      QUIET=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1 (see --help)"
      ;;
  esac
done

need_cmd curl
need_cmd tar
need_cmd awk
need_cmd grep

ASSET="$(detect_asset)"
TARGET_DIR="$(select_bin_dir)"

if [ -n "$VERSION" ]; then
  BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
  RELEASE_LABEL="$VERSION"
else
  BASE_URL="https://github.com/$REPO/releases/latest/download"
  RELEASE_LABEL="latest"
fi

TARBALL_URL="$BASE_URL/$ASSET"
CHECKSUM_URL="$BASE_URL/checksums.txt"

TMPDIR="$(mktemp -d 2>/dev/null || mktemp -d -t qmux-install)"
trap 'rm -rf "$TMPDIR"' EXIT INT TERM

TARBALL="$TMPDIR/$ASSET"
CHECKSUMS="$TMPDIR/checksums.txt"

log "Installing $BIN_NAME ($RELEASE_LABEL) for asset: $ASSET"
log "Downloading binary..."
curl -fsSL "$TARBALL_URL" -o "$TARBALL"

if [ "$VERIFY_CHECKSUM" -eq 1 ]; then
  log "Verifying checksum..."
  curl -fsSL "$CHECKSUM_URL" -o "$CHECKSUMS"
  target_dir="$(printf '%s' "$ASSET" | sed -e 's/^qmux-//' -e 's/\.tar\.gz$//')"
  expected_path="./$target_dir/$ASSET"
  expected="$(
    awk -v expected_path="$expected_path" '
      {
        if ($2 == expected_path) {
          print $1
          exit
        }
      }
    ' "$CHECKSUMS"
  )"
  [ -n "$expected" ] || die "checksum entry not found for $expected_path"
  actual="$(sha256_file "$TARBALL")"
  [ "$expected" = "$actual" ] || die "checksum mismatch for $ASSET"
fi

log "Extracting..."
tar -xzf "$TARBALL" -C "$TMPDIR"
[ -f "$TMPDIR/$BIN_NAME" ] || die "archive does not contain '$BIN_NAME'"

mkdir -p "$TARGET_DIR"
DEST="$TARGET_DIR/$BIN_NAME"
if command -v install >/dev/null 2>&1; then
  install -m 0755 "$TMPDIR/$BIN_NAME" "$DEST"
else
  cp "$TMPDIR/$BIN_NAME" "$DEST"
  chmod 0755 "$DEST"
fi

log "Installed: $DEST"
log "Version: $("$DEST" --version 2>/dev/null || echo 'installed')"

case ":${PATH:-}:" in
  *":$TARGET_DIR:"*) ;;
  *)
    log ""
    log "Add '$TARGET_DIR' to PATH if needed:"
    log "  export PATH=\"$TARGET_DIR:\$PATH\""
    ;;
esac
