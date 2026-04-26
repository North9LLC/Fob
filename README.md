<div align="center">

# NorthUSB

**Turn any USB drive into an encrypted vault.**

Passwords · TOTP codes · SSH keys · Secure notes — all in a single encrypted file on your USB.

</div>

---

## Setup

Plug in a USB drive, then run:

```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh
```

The script detects your USB drive and copies the vault UI to it. If it finds an existing NorthUSB vault, it offers to **update** (keep your data) or **wipe** (start fresh).

That's it. Nothing is installed on your computer. Your vault lives on the USB.

---

## Usage

Open `index.html` from your USB drive in any browser. Create a vault with a passphrase, add your entries, and lock when done. Saves happen automatically — no dialogs.

To update the vault UI on your USB later:

```sh
curl -fsSL https://raw.githubusercontent.com/North9LLC/NorthUSB/main/install/install.sh | sh
```

Run the same script. It detects the existing vault and offers the update option.

---

## Browser support

The vault runs entirely client-side — no server, no network, no WASM. Works in any modern browser.

| Browser | Auto-saves | Notes |
|---|:---:|---|
| Chrome / Edge 86+ | ✅ | |
| Firefox | ✅ | |
| Safari 15.4+ | ✅ | |
| Brave | ✅ | |

Vault data is stored in the browser's IndexedDB. Use **Export to USB** in the sidebar to write a `vault.sigil` file to your drive for backup or cross-browser use.

---

## Security model

### Vault format

```
vault.sigil
├── Header (56 bytes): magic · version · iterations · salt · IV
└── Body: AES-256-GCM ciphertext (WebCrypto)
```

### Cryptographic primitives

| Component | Algorithm |
|---|---|
| Encryption | AES-256-GCM |
| Key derivation | PBKDF2-HMAC-SHA256 — 310,000 iterations |
| TOTP | RFC 6238 — HMAC-SHA1 |

### Memory

- Passphrase is zero-filled from the typed `Uint8Array` on vault lock.
- Clipboard is automatically cleared 30 seconds after any copy.
- CSP blocks all outbound network requests (`connect-src 'none'`).

---

## Build from source

Requires Rust 1.75+.

```sh
git clone https://github.com/North9LLC/NorthUSB.git
cd NorthUSB
cargo build --release -p sigil-cli
```

---

## Contributing

Issues and pull requests welcome. For security vulnerabilities, open a [private advisory](https://github.com/North9LLC/NorthUSB/security/advisories/new).

---

## License

MIT OR Apache-2.0
