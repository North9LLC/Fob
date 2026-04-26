<div align="center">

# 🛡️ Sigil — NorthUSB

**Turn any USB drive into a hardware-backed encrypted vault.**

Passwords · TOTP · SSH keys · Secure notes · Files — all in a single encrypted file.

[![CI](https://github.com/North9LLC/NorthUSB/actions/workflows/ci.yml/badge.svg)](https://github.com/North9LLC/NorthUSB/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/North9LLC/NorthUSB?color=blue)](https://github.com/North9LLC/NorthUSB/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-green)](LICENSE)

</div>

---

## Quick start

### Option A — Browser vault (no install required)

The web interface is a single self-contained HTML file. No server, no dependencies, no installation.

```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/web/index.html \
  -o sigil.html
open sigil.html          # macOS
# xdg-open sigil.html   # Linux
```

Or open it directly in your browser — it works from any location, including off a USB drive.

> **Chrome or Edge recommended** for the smoothest experience. See [Browser compatibility](#browser-compatibility) for details.

### Option B — Full CLI install (USB formatting + TUI)

```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh
```

After install, run `sigil init` to create your first vault. The CLI formats your USB, sets up the encrypted vault, and writes `sigil.html` to the drive so it's always with your data.

**Inspect before running:**
```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh > install.sh
cat install.sh    # read it
sh install.sh     # run it when satisfied
```

**Options:**
```sh
# Specific version
curl ... | sh -s -- --version=v0.2.0

# Don't modify PATH
curl ... | sh -s -- --no-path

# Temp install (auto-removed on shell exit)
curl ... | sh -s -- --temp
```

---

## Features

| Feature | Details |
|---|---|
| **Encrypted vault** | XChaCha20-Poly1305 AEAD · 16 MiB fixed-size file |
| **Strong KDF** | Argon2id — 256 MiB RAM, 4 iterations, 4 lanes |
| **Plausible deniability** | Decoy slot: a second full vault behind a different passphrase |
| **Duress wipe** | Enter a third passphrase → vault is securely overwritten, returns a generic error |
| **Password manager** | Username, URL, notes per entry · clipboard auto-clears after 30 s |
| **TOTP** | RFC 6238 — SHA-1/256/512 · live countdown in TUI and browser |
| **SSH keys** | Store Ed25519/RSA keys, serve via built-in SSH agent |
| **Encrypted files** | Store arbitrary files inside the vault |
| **Secure notes** | Encrypted text with timestamps |
| **Terminal UI** | Keyboard-driven ratatui interface · auto-lock after 15 min inactivity |
| **Browser UI** | Pure WebCrypto, no server, no WASM, single HTML file |
| **Auto-lock** | Locks on inactivity (TUI: configurable, browser: 15 min) |
| **Auto-updates** | `sigil update` checks GitHub and installs the latest release |

---

## Browser compatibility

The browser vault runs entirely client-side — no server, no network requests, no WASM. Your vault data never leaves your device.

| Browser | Auto-saves to USB | Remembers vault | Notes |
|---|:---:|:---:|---|
| **Chrome / Edge 86+** | ✅ Silent | ✅ Auto-loads | Best experience |
| **Firefox** | 💾 Save dialog | ❌ | Works great |
| **Safari 15.2+** | 💾 Save dialog | ❌ | Works great |
| **Safari < 15.2** | ⬇️ Download | ❌ | Functional |

**Auto-saves to USB** — Chrome and Edge use the [File System Access API](https://developer.mozilla.org/en-US/docs/Web/API/File_System_Access_API) to write changes directly back to `vault.sigil` on your USB with no dialog. Other browsers show a native Save dialog on first change; subsequent saves in the same session are silent.

**Remembers vault** — On Chrome/Edge, Sigil stores the file reference in [IndexedDB](https://developer.mozilla.org/en-US/docs/Web/API/IndexedDB_API). On next open, the app skips the file picker and goes straight to the passphrase prompt (or a one-tap "Connect" button if permission needs re-granting). Firefox and Safari don't persist file handles across sessions, so the picker appears each time.

> **Firefox / Safari users:** the vault UI is fully functional — you just pick the file once per session. For a seamless experience, use Chrome or Edge, or run `sigil serve` *(coming soon — local HTTP server mode that works in every browser)*.

---

## Updating

```sh
sigil update           # check for and install the latest version
sigil update --check   # check only, don't install
```

Or re-run the install script at any time — it installs over an existing install safely:
```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh
```

Releases follow [semantic versioning](https://semver.org). Every release binary is signed with [Sigstore cosign](https://docs.sigstore.dev). Checksums are published in `SHA256SUMS` on each release.

---

## CLI usage

### First-time setup

```sh
sigil init
```

The setup wizard will:
1. Ask for a master passphrase (and confirmation).
2. Optionally configure a decoy vault with its own passphrase.
3. Optionally configure a duress-wipe passphrase.
4. Detect your USB drives and write the vault.

### Unlock (TUI)

```sh
sigil unlock              # auto-detect USB
sigil unlock /dev/sdb     # specify device
sigil -d /dev/sdb unlock
```

### List USB drives

```sh
sigil devices
```

### SSH agent

```sh
sigil agent &
export SSH_AUTH_SOCK=~/.sigil/agent.sock
ssh git@github.com
```

### Keyboard shortcuts (TUI)

| Key | Action |
|---|---|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `J` / `K` | Next / previous section |
| `/` | Search |
| `y` | Copy to clipboard |
| `p` | Reveal secret |
| `l` | Lock vault |
| `q` | Lock and quit |
| `a` | Add entry |

---

## Security model

### Vault format

Every vault is a fixed **16 MiB** file — all slots are the same size and always contain either real ciphertext or random noise, making it impossible to determine how many slots are active without the key.

```
vault.sigil (16 MiB)
├── Header (144 bytes)
│   ├── Salt (32 bytes)        — Argon2id salt, unique per vault
│   └── Nonces (4 × 24 bytes) — one XChaCha20 nonce per slot
└── Cells (4 × ~4 MiB)
    ├── Cell 0 — Main vault    (your real data)
    ├── Cell 1 — Decoy vault   (optional: a second full vault)
    ├── Cell 2 — Duress vault  (optional: triggers secure wipe)
    └── Cell 3 — Reserved
```

### Key derivation

```
passphrase + salt
      │
      ▼
  Argon2id (256 MiB, 4 iters, 4 lanes) — 64-byte output
      │
      ├─ HKDF-SHA256("sigil/v1/main")     → slot 0 key (32 bytes)
      ├─ HKDF-SHA256("sigil/v1/decoy")    → slot 1 key
      ├─ HKDF-SHA256("sigil/v1/duress")   → slot 2 key
      └─ HKDF-SHA256("sigil/v1/reserved") → slot 3 key
```

Every passphrase attempt derives all four keys and attempts all three active slots simultaneously — timing is identical whether you enter the main, decoy, or a wrong passphrase.

### Cryptographic primitives

| Component | Algorithm |
|---|---|
| CLI vault encryption | XChaCha20-Poly1305 (24-byte nonce) |
| Browser vault encryption | AES-256-GCM (WebCrypto API) |
| KDF (CLI) | Argon2id — 256 MiB, 4 iterations, 4 lanes |
| KDF (browser) | PBKDF2-HMAC-SHA256 — 310,000 iterations |
| Key derivation | HKDF-SHA256 with domain-separated labels |
| TOTP | RFC 6238 — HMAC-SHA1/256/512 |
| Key comparison | `subtle::ConstantTimeEq` (timing-safe) |

### Memory security

- Passphrases and slot keys are zeroized immediately after use (`zeroize` crate).
- `mlock(2)` prevents key material from being swapped to disk.
- `MADV_DONTDUMP` excludes key pages from core dumps (Linux).
- `PR_SET_DUMPABLE(0)` disables `/proc/<pid>/mem` access (Linux).
- Browser passphrase is zero-filled from the `Uint8Array` on vault lock.
- Clipboard is automatically overwritten 30 seconds after any copy.

### Duress wipe

If the duress passphrase is entered:
1. All vault cells are overwritten with cryptographically random bytes.
2. `flush()` + `fsync()` ensure the write reaches storage.
3. A generic "decrypt failed" error is returned — indistinguishable from a wrong passphrase.

---

## Supported platforms

| Platform | Architecture | Binary |
|---|---|---|
| Linux | x86_64 (musl, static) | `sigil-*-x86_64-unknown-linux-musl.tar.gz` |
| Linux | aarch64 | `sigil-*-aarch64-unknown-linux-gnu.tar.gz` |
| Linux | ARMv7 | `sigil-*-armv7-unknown-linux-gnueabihf.tar.gz` |
| macOS | x86_64 | `sigil-*-x86_64-apple-darwin.tar.gz` |
| macOS | Apple Silicon | `sigil-*-aarch64-apple-darwin.tar.gz` |
| Windows | — | Planned |

---

## Build from source

Requires Rust 1.75+.

```sh
git clone https://github.com/North9LLC/NorthUSB.git
cd NorthUSB
cargo build --release -p sigil-cli
./target/release/sigil --help
```

Run the test suite:
```sh
cargo test --workspace
```

---

## Contributing

Issues and pull requests welcome. For security vulnerabilities, please open a [private advisory](https://github.com/North9LLC/NorthUSB/security/advisories/new) instead of a public issue.

---

## License

MIT OR Apache-2.0 — your choice.
