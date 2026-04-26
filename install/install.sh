#!/usr/bin/env sh
# NorthUSB — vault setup
# ──────────────────────────────────────────────────────────────────────────────
# curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh
#
# Copies the NorthUSB vault UI to a USB drive.
# Nothing is installed on your computer — your encrypted vault lives on the USB.
# ──────────────────────────────────────────────────────────────────────────────
set -eu

NORTHUSB_URL="https://raw.githubusercontent.com/North9LLC/NorthUSB/main/web/index.html"
NORTHUSB_MARKER=".northusb"
TMPFILE=""

cleanup() { [ -n "$TMPFILE" ] && rm -f "$TMPFILE"; }
trap cleanup EXIT

# ── output helpers ────────────────────────────────────────────────────────────
die() { printf '\n  ERROR: %s\n\n' "$*" >&2; exit 1; }
say() { printf '  %s\n' "$*"; }
hr()  { printf '  ──────────────────────────────────────────\n'; }

REPLY=""
ask() {
  # ask "prompt" "default"  — reads from TTY even when stdin is the curl pipe
  printf '  %s ' "$1"
  IFS= read -r REPLY </dev/tty 2>/dev/null || REPLY=""
  [ -z "$REPLY" ] && REPLY="${2:-}"
}

# ── platform detection ────────────────────────────────────────────────────────
OS="$(uname -s)"
case "$OS" in
  Darwin|Linux) ;;
  *) die "Unsupported OS: $OS. NorthUSB supports macOS and Linux." ;;
esac

# ── USB volume detection ──────────────────────────────────────────────────────
# Prints one mount-point path per line.
list_usb_volumes() {
  case "$OS" in
    Darwin)
      # diskutil list external physical → /dev/diskN entries; mount → paths
      # awk extracts just the device path (strips " (external, physical):" suffix)
      diskutil list external physical 2>/dev/null \
        | grep "^/dev/" \
        | awk '{print $1}' \
        | while IFS= read -r disk; do
            mount 2>/dev/null | grep "^${disk}" | sed 's/.* on //; s/ (.*//'
          done
      ;;
    Linux)
      if command -v lsblk >/dev/null 2>&1; then
        lsblk -rno MOUNTPOINT,RM,TYPE 2>/dev/null \
          | awk '$3=="part" && $2=="1" && $1!="" && $1!="[SWAP]" {print $1}'
      else
        for base in "/media/$USER" /media /run/media; do
          [ -d "$base" ] || continue
          find "$base" -maxdepth 2 -mindepth 1 -type d 2>/dev/null
          break
        done
      fi
      ;;
  esac
}

# ── detect existing NorthUSB install ─────────────────────────────────────────
has_northusb() {
  vol="$1"
  [ -f "${vol}/${NORTHUSB_MARKER}" ] && return 0
  grep -qs 'sigil-vault-data\|NorthUSB' "${vol}/index.html" 2>/dev/null && return 0
  return 1
}

# ── download ──────────────────────────────────────────────────────────────────
download_index() {
  TMPFILE="$(mktemp /tmp/northusb-XXXXXX.html)"
  say "Downloading latest vault UI..."
  curl -fsSL --max-time 30 -o "$TMPFILE" "$NORTHUSB_URL" \
    || die "Download failed. Check your internet connection."
  grep -qs 'sigil-vault-data\|NorthUSB' "$TMPFILE" \
    || die "Downloaded file looks wrong. Try again."
  say "Download OK ✓"
}

# ── copy to USB ───────────────────────────────────────────────────────────────
install_to_vol() {
  vol="$1"
  mode="$2"   # fresh | update | wipe

  [ -w "$vol" ] || die "Cannot write to $vol — check disk permissions or try with sudo."

  if [ "$mode" = "wipe" ]; then
    say "Removing existing NorthUSB files..."
    rm -f "${vol}/index.html" "${vol}/${NORTHUSB_MARKER}" "${vol}/vault.sigil"
  fi

  cp "$TMPFILE" "${vol}/index.html"
  touch "${vol}/${NORTHUSB_MARKER}"

  # Hide the marker file so it doesn't clutter the drive
  if [ "$OS" = "Darwin" ]; then
    chflags hidden "${vol}/${NORTHUSB_MARKER}" 2>/dev/null || true
  elif [ "$OS" = "Linux" ]; then
    : # dot-prefix is already hidden on Linux
  fi

  printf '\n'
  hr
  say "✓  NorthUSB on: $vol"
  hr
  case "$mode" in
    wipe)   say "Open index.html from your USB in a browser to create a new vault." ;;
    update) say "Vault UI updated. Your vault data is untouched."
            say "Open index.html from your USB in a browser." ;;
    fresh)  say "Open index.html from your USB in a browser to get started." ;;
  esac
  printf '\n'
}

# ── numbered list picker ──────────────────────────────────────────────────────
# Sets REPLY to the selected mount-point.
pick_volume() {
  vols="$1"
  count=$(printf '%s\n' "$vols" | grep -c '.')
  i=1
  while IFS= read -r v; do
    [ -n "$v" ] || continue
    printf '    [%d]  %s\n' "$i" "$v"
    i=$((i + 1))
  done <<EOF
$vols
EOF
  printf '\n'
  if [ "$count" = "1" ]; then
    REPLY=$(printf '%s\n' "$vols" | head -1)
    return
  fi
  ask "Select drive [1]:" "1"
  sel="$REPLY"
  REPLY=$(printf '%s\n' "$vols" | sed -n "${sel}p")
  [ -n "$REPLY" ] || die "Invalid selection."
}

# ── banner ────────────────────────────────────────────────────────────────────
print_banner() {
  printf '\n'
  hr
  printf '  NorthUSB — encrypted vault setup\n'
  hr
  printf '\n'
}

# ── main ──────────────────────────────────────────────────────────────────────
main() {
  print_banner

  # Collect all mounted USB volumes into a newline-separated list
  all_vols=""
  while IFS= read -r v; do
    [ -n "$v" ] || continue
    all_vols="${all_vols:+${all_vols}
}$v"
  done <<EOF
$(list_usb_volumes)
EOF

  if [ -z "$all_vols" ]; then
    say "No USB drives found."
    say ""
    say "Make sure your USB drive is plugged in and mounted, then re-run:"
    say ""
    say "  curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh"
    say ""
    say "If your drive is plugged in but not showing up, open Finder to"
    say "make sure it's mounted, then try again."
    printf '\n'
    exit 0
  fi

  # Partition into "has NorthUSB" vs "fresh"
  north_vols=""
  fresh_vols=""
  while IFS= read -r v; do
    [ -n "$v" ] || continue
    if has_northusb "$v"; then
      north_vols="${north_vols:+${north_vols}
}$v"
    else
      fresh_vols="${fresh_vols:+${fresh_vols}
}$v"
    fi
  done <<EOF
$all_vols
EOF

  # ── Existing NorthUSB vault ────────────────────────────────────────────────
  if [ -n "$north_vols" ]; then
    say "NorthUSB vault found on:"
    printf '\n'
    pick_volume "$north_vols"
    target="$REPLY"

    hr
    say "[1]  Update  — install the latest UI, keep your vault data"
    say "[2]  Wipe    — erase everything and start fresh"
    say "[3]  Cancel"
    hr
    ask "Choice [1]:" "1"

    case "$REPLY" in
      2)
        printf '\n'
        say "This will permanently erase all NorthUSB files on $target"
        say "(including vault.sigil if present)."
        printf '\n'
        ask "Type YES to confirm:" ""
        if [ "$REPLY" = "YES" ]; then
          download_index
          install_to_vol "$target" "wipe"
        else
          say "Cancelled."
          printf '\n'
        fi
        ;;
      3)
        say "Cancelled."
        printf '\n'
        ;;
      *)
        download_index
        install_to_vol "$target" "update"
        ;;
    esac

  # ── Fresh USB ──────────────────────────────────────────────────────────────
  elif [ -n "$fresh_vols" ]; then
    fresh_count=$(printf '%s\n' "$fresh_vols" | grep -c '.')

    if [ "$fresh_count" -gt 1 ]; then
      say "Multiple USB drives found. Pick one to set up:"
      printf '\n'
      pick_volume "$fresh_vols"
      target="$REPLY"
    else
      target="$fresh_vols"
      say "USB drive found: $target"
    fi

    printf '\n'
    ask "Set up NorthUSB on $target? [Y/n]:" "y"

    case "$REPLY" in
      [Nn]*)
        say "Cancelled."
        printf '\n'
        ;;
      *)
        download_index
        install_to_vol "$target" "fresh"
        ;;
    esac
  fi
}

main "$@"
