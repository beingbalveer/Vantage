# Contributing to Vantage

Thank you for your interest in contributing to Vantage!

## Prerequisites

1. **Rust:** Stable toolchain (`rustc`, `cargo` edition 2021+). [Install Rust](https://rustup.rs/).
2. **Node.js:** Node 18+ and `npm`. [Install Node.js](https://nodejs.org/).
3. **Git:** System `git` CLI installed and available on `PATH`.

### Linux Dependencies
On Debian/Ubuntu-based distributions, install the required system development packages:
```bash
sudo apt-get update && sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

---

## Running Locally

```bash
# Install frontend dependencies
npm install

# Start the Tauri development window with hot reload
npm run dev:app

# Or build a real "Vantage Dev" app (amber icon, same identifier as the release
# app so it opens your existing repos and settings)
npm run dev:bundle
```

Dev runs are named **Vantage Dev**, use an amber icon and show a `DEV` chip in
the header, so they are never mistaken for the released app. `npm run tauri dev`
still works when you do not need the dev identity.

---

## Testing & Quality Checks

Before submitting a pull request, please ensure all checks pass:

```bash
# Run frontend unit tests (Vitest)
npm test

# Check frontend types and templates
npm run check

# Build frontend production bundle (Vite)
npm run build

# Run Rust backend unit tests
cargo test --manifest-path src-tauri/Cargo.toml

# Run Rust strict linter
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

# Check Rust code formatting
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

---

## Building Release Installers

To package native installers for your operating system:

```bash
npm run tauri build
```

Compiled bundles will be generated in `src-tauri/target/release/bundle/`.
