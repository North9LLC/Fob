# Contributing to Fob

Thank you for your interest in contributing to Fob.

## Getting started

Requires Rust 1.75+.

```sh
git clone https://github.com/North9LLC/Fob
cd Fob
cargo test --workspace --all-targets
cargo clippy --all-targets -- -D warnings
```

## Guidelines

- Zero clippy warnings (`-D warnings`) required before merge — CI enforces this
- All cryptographic changes must include tests in `fob-core`
- `fob-core` must remain pure logic — no filesystem, no network, no I/O of any kind
- Changes to the vault format (`src/format.rs`) are breaking changes — discuss in an issue first
- The CLI, agent, and browser vault must not handle raw key material — passphrases go directly to `fob-core` and are zeroized immediately after use

## Architecture rule

```
fob-core  ← no I/O, pure crypto logic, fully testable
fob-cli   ← calls fob-core, owns TUI and USB device management
fob-agent ← SSH agent + TOTP daemon, calls fob-core
fob-stego ← steganographic cover formats, calls fob-core
```

Do not add I/O to `fob-core`. Do not add cryptographic logic to `fob-cli`.

## Security

For security vulnerabilities, open a [private advisory](https://github.com/North9LLC/Fob/security/advisories/new) — not a public issue. Do not disclose vulnerabilities publicly until a fix is available.

## License

Fob is licensed under MIT OR Apache-2.0. By contributing, you agree your contributions will be licensed under the same terms.
