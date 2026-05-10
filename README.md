<div align="center">

# Fob

**Your secrets, on your keychain.**

An encrypted vault that lives on a USB drive — passwords, TOTP codes, SSH keys, and secure notes, protected by Argon2id and XChaCha20-Poly1305. Nothing installed on your computer.

[![CI](https://github.com/North9LLC/Fob/actions/workflows/ci.yml/badge.svg)](https://github.com/North9LLC/Fob/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust 1.75+](https://img.shields.io/badge/rust-1.75+-orange.svg)](#building-from-source)

</div>

---

Fob turns any USB stick into a cryptographic security key. Plug it in, unlock with a passphrase, and your credentials are available as a password manager, TOTP generator, and SSH agent. Unplug and everything locks.

---

## What's in the vault

- **Passwords** — store, generate, and auto-copy credentials
- **TOTP** — built-in two-factor code generation with live countdown
- **SSH keys** — unlocked keys exposed via a local Unix socket, compatible with any SSH client
- **Secure notes** — encrypted free-text entries
- **Plausible deniability** — decoy vault slot with realistic fake data; duress slot that destroys the vault silently
- **Browser vault** — a zero-dependency HTML file that runs entirely offline, same encrypted format

---

## Security

### Cryptographic primitives

| Component | Algorithm |
|---|---|
| Key derivation | Argon2id — 256 MB memory, 4 iterations, 4 lanes |
| Encryption (CLI) | XChaCha20-Poly1305 |
| Encryption (browser) | AES-256-GCM via WebCrypto |
| Key separation | HKDF-SHA256 |
| Post-quantum (optional) | ML-KEM-1024 hybrid wrapping |
| TOTP | RFC 6238 — HMAC-SHA1 |

### Threat model

| Threat | Mitigation |
|---|---|
| USB stolen | Argon2id makes brute-force economically infeasible |
| Coercion | Decoy vault opens with realistic fake data |
| Extreme coercion | Duress passphrase silently destroys the vault |
| Quantum adversary | Optional ML-KEM-1024 hybrid key wrapping |
| Clipboard exfil | Auto-clears 30 seconds after any copy |
| Memory dumps | Sensitive buffers zeroized and mlocked where possible |

### Architecture

All cryptographic operations live in `fob-core`, which has no filesystem or network access. The CLI and browser vault cannot leak key material because they never handle raw secrets — passphrases are passed directly to the crypto layer and zeroized immediately after use.

---

## Building from source

Requires Rust 1.75+.

```sh
git clone https://github.com/North9LLC/Fob.git
cd Fob
cargo build --release -p fob-cli
```

The binary lands at `target/release/fob`.

---

## Repository layout

```
fob/
├── crates/
│   ├── fob-core/       # cryptography and vault format — no I/O, pure logic
│   ├── fob-cli/        # TUI + USB device management
│   ├── fob-agent/      # SSH agent + TOTP daemon
│   └── fob-stego/      # steganographic cover formats
├── install/
│   └── install.sh      # one-line installer
└── web/
    └── index.html      # zero-dependency browser vault
```

---

## Contributing

Issues and pull requests welcome. For security vulnerabilities, please open a [private advisory](https://github.com/North9LLC/Fob/security/advisories/new) rather than a public issue.

---

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) at your option.
