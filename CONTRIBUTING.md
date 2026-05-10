# Contributing to Fob

Thanks for your interest in contributing! Fob is a security tool, so we hold contributions to a high standard of correctness and clarity.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOURNAME/fob.git`
3. Create a branch: `git checkout -b feature/your-feature`
4. Make your changes
5. Run the checks: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
6. Commit and push
7. Open a pull request

## Development Setup

Requires **Rust 1.75+**.

```sh
cargo build --workspace
cargo test --workspace
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and resolve all warnings
- Prefer explicit error handling over `unwrap()`/`expect()` in non-test code
- All key material types must implement `Zeroize` and `ZeroizeOnDrop`
- Use `subtle::ConstantTimeEq` for all secret comparisons — never `==` on bytes
- Write tests for security-critical code (especially in `fob-core`)
- Keep `fob-core` free of filesystem, network, and terminal I/O

## Commit Messages

Use clear, imperative commit messages:

```
Add Argon2id parameter validation
Fix off-by-one in vault header parsing
Update browser vault to use AES-256-GCM
```

## Pull Request Process

1. Ensure CI passes (formatting, clippy, tests)
2. Update relevant documentation if behavior changes
3. For security-sensitive changes, expect more review time
4. Squash fix-up commits before final review if requested

## Areas That Need Help

- Cross-platform USB device detection improvements
- Windows support and PowerShell installer
- Browser vault accessibility (a11y) enhancements
- Additional steganographic cover formats
- Fuzzing corpus expansion

## Code of Conduct

Be respectful, constructive, and patient. Security code is hard — questions and challenges are welcome when they're in good faith.
