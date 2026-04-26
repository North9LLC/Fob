#!/usr/bin/env sh
# NorthUSB — installer
# ──────────────────────────────────────────────────────────────────────────────
# curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh
#
# Downloads the sigil binary and runs the interactive TUI setup wizard.
# Falls back to building from source if no release is published yet.
#
# Flags:
#   --version=vX.Y.Z   install a specific release (default: latest)
#   --no-path          skip adding ~/.sigil/bin to your shell rc
#   --local=/path      use a locally built binary instead of downloading
# ──────────────────────────────────────────────────────────────────────────────
set -eu

SIGIL_INSTALL_DIR="${HOME}/.sigil/bin"
SIGIL_RELEASES_API="https://api.github.com/repos/North9LLC/NorthUSB/releases/latest"
SIGIL_BASE_URL="https://github.com/North9LLC/NorthUSB/releases/download"

VERSION=""
MODIFY_PATH=1
LOCAL_BIN=""

for arg in "$@"; do
  case "$arg" in
    --version=*)  VERSION="${arg#*=}" ;;
    --no-path)    MODIFY_PATH=0 ;;
    --local=*)
      LOCAL_BIN="${arg#*=}"
      case "$LOCAL_BIN" in
        "~/"*) LOCAL_BIN="${HOME}/${LOCAL_BIN#~/}" ;;
      esac
      ;;
    *)            printf 'Unknown flag: %s\n' "$arg" >&2; exit 1 ;;
  esac
done

# ── helpers ───────────────────────────────────────────────────────────────────
die() { printf '\n  ERROR: %s\n\n' "$*" >&2; exit 1; }
say() { printf '  %s\n' "$*"; }
hr()  { printf '  ──────────────────────────────────────────\n'; }

# ── platform ──────────────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      x86_64) PLATFORM="x86_64-apple-darwin" ;;
      arm64)  PLATFORM="aarch64-apple-darwin" ;;
      *)      die "Unsupported architecture: $ARCH" ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64)  PLATFORM="x86_64-unknown-linux-musl" ;;
      aarch64) PLATFORM="aarch64-unknown-linux-gnu" ;;
      armv7l)  PLATFORM="armv7-unknown-linux-gnueabihf" ;;
      *)       die "Unsupported architecture: $ARCH" ;;
    esac
    ;;
  *)
    die "Unsupported OS: $OS. NorthUSB supports macOS and Linux." ;;
esac

# ── banner ────────────────────────────────────────────────────────────────────
printf '\n'
hr
printf '  NorthUSB — Encrypted USB Vault\n'
hr
printf '\n'

# ── version resolution ────────────────────────────────────────────────────────
if [ -n "$LOCAL_BIN" ]; then
  [ -x "$LOCAL_BIN" ] || die "Local binary not found or not executable: $LOCAL_BIN"
  say "Using local build: $LOCAL_BIN"
  VERSION="local"
elif [ -f "Cargo.toml" ] && [ -z "$VERSION" ]; then
  # Running from inside the repo — build from source directly.
  if ! command -v cargo >/dev/null 2>&1; then
    die "cargo not found. Install Rust from https://rustup.rs and retry."
  fi
  say "Building from source..."
  cargo build --release -p sigil-cli 2>&1 | tail -3
  [ -f "target/release/sigil" ] || die "Build failed."
  LOCAL_BIN="$(pwd)/target/release/sigil"
  VERSION="local"
elif [ -z "$VERSION" ]; then
  say "Fetching latest release..."
  VERSION="$(curl -fsSL --max-time 10 "$SIGIL_RELEASES_API" \
    | grep '"tag_name"' \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')" || true

  if [ -z "$VERSION" ]; then
    die "No release found and not in a source repo.
  Clone the repo and re-run:
    git clone https://github.com/North9LLC/NorthUSB.git
    cd NorthUSB && sh install/install.sh"
  fi
  say "Version: $VERSION"
fi

# ── download / copy binary ────────────────────────────────────────────────────
TMPDIR_WORK="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_WORK"' EXIT

if [ -n "$LOCAL_BIN" ]; then
  cp "$LOCAL_BIN" "${TMPDIR_WORK}/sigil"
else
  ARTIFACT="sigil-${VERSION}-${PLATFORM}.tar.gz"
  ARTIFACT_URL="${SIGIL_BASE_URL}/${VERSION}/${ARTIFACT}"

  say "Downloading ${ARTIFACT}..."
  curl -fsSL --progress-bar --max-time 120 \
    -o "${TMPDIR_WORK}/${ARTIFACT}" "$ARTIFACT_URL" \
    || die "Download failed. Check your internet connection."

  say "Extracting..."
  tar -xzf "${TMPDIR_WORK}/${ARTIFACT}" -C "$TMPDIR_WORK" \
    || die "Extraction failed."
fi

[ -f "${TMPDIR_WORK}/sigil" ] || die "Binary not found."
chmod 755 "${TMPDIR_WORK}/sigil"

# ── install ───────────────────────────────────────────────────────────────────
mkdir -p "$SIGIL_INSTALL_DIR"
cp "${TMPDIR_WORK}/sigil" "${SIGIL_INSTALL_DIR}/sigil"
chmod 755 "${SIGIL_INSTALL_DIR}/sigil"
say "Installed to: ${SIGIL_INSTALL_DIR}/sigil"

# ── PATH setup ────────────────────────────────────────────────────────────────
if [ "$MODIFY_PATH" = "1" ]; then
  SHELL_NAME="$(basename "${SHELL:-sh}")"
  RC_FILE=""
  case "$SHELL_NAME" in
    bash) RC_FILE="${HOME}/.bashrc" ;;
    zsh)  RC_FILE="${HOME}/.zshrc" ;;
    fish) RC_FILE="${HOME}/.config/fish/config.fish" ;;
  esac

  if [ -n "$RC_FILE" ]; then
    if grep -qF "$SIGIL_INSTALL_DIR" "$RC_FILE" 2>/dev/null; then
      : # already there
    else
      if [ "$SHELL_NAME" = "fish" ]; then
        printf '\nfish_add_path %s\n' "$SIGIL_INSTALL_DIR" >> "$RC_FILE"
      else
        printf '\nexport PATH="$PATH:%s"\n' "$SIGIL_INSTALL_DIR" >> "$RC_FILE"
      fi
      say "Added to PATH in $RC_FILE"
    fi
  fi
fi

# ── launch setup wizard ───────────────────────────────────────────────────────
printf '\n'
hr
say "Launching NorthUSB setup..."
hr
printf '\n'

exec "${SIGIL_INSTALL_DIR}/sigil"
