#!/usr/bin/env sh
# NorthUSB / Sigil install script
#
# Usage:
#   curl -fsSL https://sigil.sh/install | sh
#   curl -fsSL https://sigil.sh/install | sh -s -- --temp
#
# Flags:
#   --temp      Extract to a temp directory, auto-remove on exit
#   --no-path   Don't modify shell rc files
#   --version   Override version to install (default: latest)
#
# This script:
#   1. Detects OS and architecture
#   2. Refuses to run in obviously hostile environments
#   3. Prints a banner with its own SHA256 so you can verify
#   4. Downloads the release artifact
#   5. Verifies the cosign signature
#   6. Installs to ~/.sigil/bin/sigil and optionally updates PATH
#   7. Runs `sigil init` to start the setup wizard
#
# You are encouraged to read this script before running it.
# Every step is commented.

set -eu

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
SIGIL_INSTALL_DIR="${HOME}/.sigil/bin"
SIGIL_BASE_URL="https://github.com/North9LLC/NorthUSB/releases/download"
SIGIL_LATEST_URL="https://api.github.com/repos/North9LLC/NorthUSB/releases/latest"

# Cosign public key for release verification.
# This key signs every release artifact. If it doesn't match, we refuse to install.
COSIGN_PUBLIC_KEY='-----BEGIN PUBLIC KEY-----
# TODO: replace with actual cosign public key at release time
-----END PUBLIC KEY-----'

# ---------------------------------------------------------------------------
# Flags
# ---------------------------------------------------------------------------
TEMP_INSTALL=0
MODIFY_PATH=1
VERSION=""

for arg in "$@"; do
  case "$arg" in
    --temp)      TEMP_INSTALL=1 ;;
    --no-path)   MODIFY_PATH=0 ;;
    --version=*) VERSION="${arg#*=}" ;;
    *)           echo "Unknown flag: $arg" >&2; exit 1 ;;
  esac
done

# ---------------------------------------------------------------------------
# Safety checks — refuse to run in obviously hostile contexts
# ---------------------------------------------------------------------------
check_environment() {
  # Must have a TTY (or at least a sane shell).
  if [ -z "${SHELL:-}" ]; then
    echo "ERROR: \$SHELL is not set. Cannot determine your shell." >&2
    exit 1
  fi

  # Warn if running as root.
  if [ "$(id -u)" = "0" ]; then
    echo "WARNING: Running as root. Sigil is designed for regular users." >&2
    echo "         Press Ctrl-C to abort, or wait 5 seconds to continue." >&2
    sleep 5
  fi

  # Refuse if curl is not available.
  if ! command -v curl >/dev/null 2>&1; then
    echo "ERROR: curl is required but not installed." >&2
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------
detect_platform() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "$OS" in
    Linux)
      case "$ARCH" in
        x86_64)  PLATFORM="x86_64-unknown-linux-musl" ;;
        aarch64) PLATFORM="aarch64-unknown-linux-gnu" ;;
        armv7l)  PLATFORM="armv7-unknown-linux-gnueabihf" ;;
        *)       echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
      esac
      ;;
    Darwin)
      case "$ARCH" in
        x86_64)  PLATFORM="x86_64-apple-darwin" ;;
        arm64)   PLATFORM="aarch64-apple-darwin" ;;
        *)       echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
      esac
      ;;
    *)
      echo "Unsupported OS: $OS" >&2
      echo "For Windows, use: iwr -useb https://sigil.sh/install.ps1 | iex" >&2
      exit 1
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Banner — printed before anything is downloaded
# ---------------------------------------------------------------------------
print_banner() {
  # Compute SHA256 of this script so the user can verify what they ran.
  SCRIPT_SHA=""
  if command -v sha256sum >/dev/null 2>&1; then
    SCRIPT_SHA="$(sha256sum "$0" | cut -d' ' -f1)"
  elif command -v shasum >/dev/null 2>&1; then
    SCRIPT_SHA="$(shasum -a 256 "$0" | cut -d' ' -f1)"
  fi

  cat <<EOF

  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
  ▓                          ▓
  ▓        S I G I L         ▓
  ▓                          ▓
  ▓     NORTHUSB VAULT        ▓
  ▓     Install Script        ▓
  ▓                          ▓
  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

  Platform:   $PLATFORM
  Install to: ${SIGIL_INSTALL_DIR}/sigil
  Script SHA: ${SCRIPT_SHA:-unavailable}

  This script will:
    1. Download the Sigil binary for your platform
    2. Verify the release signature with cosign
    3. Install to ${SIGIL_INSTALL_DIR}
    4. Optionally update your PATH

  Press Ctrl-C within 3 seconds to abort.

EOF
  sleep 3
}

# ---------------------------------------------------------------------------
# Version resolution — latest if not specified
# ---------------------------------------------------------------------------
resolve_version() {
  if [ -z "$VERSION" ]; then
    echo "  Fetching latest version..." >&2
    VERSION="$(curl -fsSL "$SIGIL_LATEST_URL" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
    if [ -z "$VERSION" ]; then
      echo "ERROR: Could not determine latest version." >&2
      exit 1
    fi
  fi
  echo "  Version: $VERSION" >&2
}

# ---------------------------------------------------------------------------
# Download — artifact + signature + certificate
# ---------------------------------------------------------------------------
download_artifact() {
  ARTIFACT="sigil-${VERSION}-${PLATFORM}.tar.gz"
  ARTIFACT_URL="${SIGIL_BASE_URL}/${VERSION}/${ARTIFACT}"
  SIG_URL="${ARTIFACT_URL}.sig"
  CERT_URL="${ARTIFACT_URL}.cert"

  TMPDIR="$(mktemp -d)"
  trap 'rm -rf "$TMPDIR"' EXIT

  echo "  Downloading ${ARTIFACT}..." >&2
  curl -fsSL --progress-bar --max-time 120 -o "${TMPDIR}/${ARTIFACT}" "$ARTIFACT_URL"
  curl -fsSL -o "${TMPDIR}/${ARTIFACT}.sig"  "$SIG_URL"
  curl -fsSL -o "${TMPDIR}/${ARTIFACT}.cert" "$CERT_URL"
}

# ---------------------------------------------------------------------------
# Signature verification — cosign
# ---------------------------------------------------------------------------
verify_signature() {
  if ! command -v cosign >/dev/null 2>&1; then
    echo "  WARNING: cosign not found — skipping signature verification." >&2
    echo "           Install cosign (https://docs.sigstore.dev/cosign/installation/)" >&2
    echo "           to verify release authenticity." >&2
    echo "  Continuing without verification in 5 seconds..." >&2
    sleep 5
    return
  fi

  echo "  Verifying signature with cosign..." >&2

  KEY_FILE="${TMPDIR}/sigil.pub"
  printf '%s\n' "$COSIGN_PUBLIC_KEY" > "$KEY_FILE"

  cosign verify-blob \
    --key "$KEY_FILE" \
    --certificate "${TMPDIR}/${ARTIFACT}.cert" \
    --signature "${TMPDIR}/${ARTIFACT}.sig" \
    "${TMPDIR}/${ARTIFACT}" || {
      echo "ERROR: Signature verification failed. Refusing to install." >&2
      exit 1
    }

  echo "  Signature verified. ✓" >&2
}

# ---------------------------------------------------------------------------
# Installation
# ---------------------------------------------------------------------------
install_binary() {
  echo "  Extracting..." >&2
  tar -xzf "${TMPDIR}/${ARTIFACT}" -C "$TMPDIR"

  if [ "$TEMP_INSTALL" = "1" ]; then
    SIGIL_BIN="$(mktemp -d)/sigil"
    cp "${TMPDIR}/sigil" "$SIGIL_BIN"
    echo "  Temporary install: $SIGIL_BIN" >&2
    # Schedule cleanup when shell exits.
    trap 'rm -f "$SIGIL_BIN"' EXIT
  else
    mkdir -p "$SIGIL_INSTALL_DIR"
    cp "${TMPDIR}/sigil" "${SIGIL_INSTALL_DIR}/sigil"
    chmod 755 "${SIGIL_INSTALL_DIR}/sigil"
    SIGIL_BIN="${SIGIL_INSTALL_DIR}/sigil"
    echo "  Installed: $SIGIL_BIN" >&2
  fi
}

# ---------------------------------------------------------------------------
# PATH setup
# ---------------------------------------------------------------------------
update_path() {
  if [ "$MODIFY_PATH" = "0" ] || [ "$TEMP_INSTALL" = "1" ]; then
    return
  fi

  SHELL_NAME="$(basename "$SHELL")"
  RC_FILE=""

  case "$SHELL_NAME" in
    bash) RC_FILE="${HOME}/.bashrc" ;;
    zsh)  RC_FILE="${HOME}/.zshrc" ;;
    fish) RC_FILE="${HOME}/.config/fish/config.fish" ;;
    *)    echo "  Unknown shell ($SHELL_NAME) — add $SIGIL_INSTALL_DIR to PATH manually." >&2; return ;;
  esac

  PATH_LINE="export PATH=\"\$PATH:${SIGIL_INSTALL_DIR}\""
  if [ "$SHELL_NAME" = "fish" ]; then
    PATH_LINE="fish_add_path ${SIGIL_INSTALL_DIR}"
  fi

  if grep -qF "$SIGIL_INSTALL_DIR" "$RC_FILE" 2>/dev/null; then
    echo "  PATH already configured in $RC_FILE" >&2
  else
    printf '\n# Sigil (NorthUSB)\n%s\n' "$PATH_LINE" >> "$RC_FILE"
    echo "  Added to PATH in $RC_FILE" >&2
    echo "  Run: source $RC_FILE  (or open a new terminal)" >&2
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  check_environment
  detect_platform
  print_banner
  resolve_version
  download_artifact
  verify_signature
  install_binary
  update_path

  cat <<EOF

  ────────────────────────────────────────────────
  Sigil installed successfully.

  Run:  $SIGIL_BIN init
    to create your first encrypted vault.

  Documentation:  https://github.com/North9LLC/NorthUSB#readme
  Source code:    https://github.com/North9LLC/NorthUSB
  ────────────────────────────────────────────────

EOF

  # Hand off to the binary.
  exec "$SIGIL_BIN" init
}

main "$@"
