# Contributing to Vantage

Thank you for your interest in contributing to Vantage!

## Prerequisites

1. **Rust:** Stable toolchain (`rustc`, `cargo` edition 2021+). [Install Rust](https://rustup.rs/).
2. **Git:** System `git` CLI installed and available on `PATH`.

### Linux Dependencies
GPUI needs the platform graphics, input and font packages. On Debian/Ubuntu:
```bash
sudo apt-get update && sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  libfontconfig1-dev \
  libfreetype6-dev \
  libwayland-dev \
  libxkbcommon-dev \
  libx11-xcb-dev \
  libxcb1-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  libvulkan-dev
```

---

## Running Locally

```bash
# Build the app
cargo build -p vantage

# Build and launch it
cargo run -p vantage
```

The debug binary is written to `target/debug/vantage`.

The workspace has two crates: `crates/vantage-core` (the Git engine) and
`crates/vantage` (the GPUI desktop app).

### macOS folder permissions

macOS ties the Documents/Desktop/Downloads permission to the app's code
signature. A rebuild produces a new signature, so macOS may ask again after
each build. If you keep repositories inside a protected folder and want to stop
the prompts while developing, keep the repositories outside `~/Documents`,
`~/Desktop` and `~/Downloads` (for example `~/Projects`), which needs no
permission at all.

Release builds are only remembered across versions when they are signed with a
stable identity (Developer ID), which is a separate signing/notarization setup.

---

## Testing & Quality Checks

Before submitting a pull request, please ensure all checks pass:

```bash
# Run all workspace tests
cargo test --workspace

# Run the strict linter
cargo clippy --workspace --all-targets -- -D warnings

# Check code formatting
cargo fmt --all -- --check
```

---

## Building a Release Binary

To build an optimized binary for your operating system:

```bash
cargo build --release -p vantage
```

The compiled binary is written to `target/release/vantage`.
