```
███████╗██╗ ██████╗ ██╗██╗
██╔════╝██║██╔════╝ ██║██║
███████╗██║██║  ███╗██║██║
╚════██║██║██║   ██║██║██║
███████║██║╚██████╔╝██║███████╗
╚══════╝╚═╝ ╚═════╝ ╚═╝╚══════╝

           N O R T H U S B
```

**Turn any USB stick into a production-grade encrypted security key.**

Sigil stores your passwords, TOTP codes, SSH keys, and files in a fixed-size encrypted vault that is indistinguishable from random noise. A single binary. Zero dependencies at runtime. Works on Linux, macOS, and Windows.

---

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh
```

This downloads the latest release binary for your platform, verifies the cosign signature, installs to `~/.sigil/bin/sigil`, and launches the setup wizard.

**Want to inspect before running?**
```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh > install.sh
cat install.sh          # read it
sh install.sh           # run it when satisfied
```

**One-time / temp install (no PATH modification):**
```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh -s -- --temp
```

**Install a specific version:**
```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh -s -- --version=v0.1.0
```

---

## What it does

| Feature | Details |
|---|---|
| **Encrypted vault** | XChaCha20-Poly1305 AEAD, 16 MiB fixed-size file |
| **Strong KDF** | Argon2id — 256 MiB RAM, 4 iterations, 4 lanes |
| **Plausible deniability** | Decoy slot: a second full vault behind a different passphrase |
| **Duress wipe** | Enter a 3rd passphrase → vault file is securely overwritten, returns generic error |
| **Password manager** | Username, URL, notes per entry; clipboard copy with auto-clear |
| **TOTP** | RFC 6238 — SHA-1/256/512 — shows live countdown in TUI |
| **SSH keys** | Store & serve Ed25519/RSA keys via built-in SSH agent |
| **File vault** | Encrypt arbitrary files inside the vault |
| **Notes** | Encrypted text notes |
| **Terminal UI** | Keyboard-driven ratatui interface, works in any terminal |
| **Auto-lock** | Locks after configurable inactivity (default 15 min) |

---

## Security model

### Encryption

Every vault slot is encrypted with **XChaCha20-Poly1305** using a 32-byte slot key derived from the master passphrase. The entire vault file is a fixed 16 MiB; all slots are the same size and always filled with either real ciphertext or random noise — making it impossible to determine how many slots are active or what they contain without the key.

```
Vault file (16 MiB)
├── Header (144 bytes)
│   ├── Salt (32 bytes)        — Argon2id salt, unique per vault
│   └── Nonces (4 × 24 bytes) — one XChaCha nonce per slot
└── Cells (4 × ~4 MiB)
    ├── Cell 0 — Main slot     (your real vault)
    ├── Cell 1 — Decoy slot    (optional: another full vault)
    ├── Cell 2 — Duress slot   (optional: triggers wipe)
    └── Cell 3 — Reserved
```

### Key derivation

```
passphrase + salt
      │
      ▼
  Argon2id (256 MiB, 4 iters, 4 lanes) → 64-byte output
      │
      ├─ HKDF-SHA256("sigil/v1/main")     → slot 0 key
      ├─ HKDF-SHA256("sigil/v1/decoy")    → slot 1 key
      ├─ HKDF-SHA256("sigil/v1/duress")   → slot 2 key
      └─ HKDF-SHA256("sigil/v1/reserved") → slot 3 key
```

Every passphrase attempt derives keys for all four slots simultaneously and attempts to decrypt all three active slots — so the timing is identical whether you enter the main, decoy, or a wrong passphrase.

### Duress wipe

If the duress passphrase is entered:
1. The vault file is overwritten in chunks with cryptographically random bytes.
2. `flush()` + `fsync()` are called to ensure the write hits storage.
3. A generic "decrypt failed" error is returned — identical to a wrong passphrase.

There is no way to distinguish a duress event from a wrong guess.

### Memory security

- Passphrases are zeroized immediately after use (`zeroize` crate).
- Slot keys are zeroized when the vault locks.
- `mlock(2)` is called on all key material to prevent swap.
- `MADV_DONTDUMP` is set to exclude key pages from core dumps.
- `PR_SET_DUMPABLE(0)` is set on Linux to disable `/proc/<pid>/mem` access.

### Passphrase comparison

Passphrase uniqueness validation uses `subtle::ConstantTimeEq` to avoid leaking information through timing side channels.

---

## Usage

### First run

```sh
sigil init
```

The setup wizard will:
1. Ask for a master passphrase (and confirmation).
2. Optionally configure a decoy vault with its own passphrase.
3. Optionally configure a duress wipe passphrase.
4. Write the vault file to your USB drive.

### Unlock

```sh
sigil open /dev/sdb           # specify the USB device
sigil open                    # auto-detect USB drives
```

### Keyboard shortcuts

| Key | Action |
|---|---|
| `j` / `↓` | Move cursor down |
| `k` / `↑` | Move cursor up |
| `J` / `K` | Next / previous vault section |
| `Tab` | Toggle sidebar / content focus |
| `Enter` | Select / confirm |
| `/` | Search |
| `y` | Copy to clipboard (auto-clears in 30s) |
| `p` | Reveal password |
| `l` | Lock vault (keep running) |
| `q` | Lock and quit |
| `Ctrl+C` | Force quit |

### SSH agent

```sh
sigil agent &
export SSH_AUTH_SOCK=~/.sigil/agent.sock
ssh git@github.com
```

### TOTP

Add a TOTP entry with the secret from your authenticator app. Sigil shows the live 6-digit code with a color-coded countdown bar:
- Green — more than 10 seconds remaining
- Gold — 5–10 seconds remaining
- Red / blinking — under 5 seconds remaining

---

## Build from source

Requires Rust 1.75+.

```sh
git clone https://github.com/North9LLC/NorthUSB.git
cd NorthUSB
cargo build --release
./target/release/sigil --help
```

Run tests:
```sh
cargo test --workspace
```

---

## Supported platforms

| Platform | Architecture | Status |
|---|---|---|
| Linux | x86_64 (musl) | ✓ Primary |
| Linux | aarch64 | ✓ |
| Linux | ARMv7 | ✓ |
| macOS | x86_64 | ✓ |
| macOS | Apple Silicon (M1/M2/M3) | ✓ |
| Windows | x86_64 | Planned |

---

## License

MIT OR Apache-2.0 — your choice.

---

## Contributing

Issues and pull requests welcome at [github.com/North9LLC/NorthUSB](https://github.com/North9LLC/NorthUSB).

Security issues: please open a private security advisory rather than a public issue.
