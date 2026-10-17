# Contributing to Vantage

Thank you for your interest in contributing to **Vantage**! We welcome contributions of all kinds: bug reports, documentation improvements, feature suggestions, and code contributions.

---

## Code of Conduct

All contributors and participants are expected to uphold our [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before participating.

---

## Development Setup

### 1. Prerequisites
- **Node.js**: v18.0.0 or higher, with `npm`
- **Rust**: Latest stable toolchain via [rustup](https://rustup.rs/)
- **Git**: Installed and accessible on your `$PATH`

#### Linux Prerequisites
If developing on Linux (Ubuntu / Debian), install the required system libraries:
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

### 2. Fork and Clone
```bash
git clone https://github.com/<your-username>/vantage.git
cd vantage
```

### 3. Install Dependencies
```bash
npm install
```

### 4. Run Development Server
```bash
npm run tauri dev
```
This starts the Vite frontend dev server with hot module replacement (HMR) and compiles the Rust backend into a desktop window.

---

## Testing & Quality Checks

Before submitting a pull request, ensure all tests and linter checks pass cleanly:

```bash
# 1. Run frontend unit tests (Vitest)
npm test

# 2. Check TypeScript and Svelte syntax
npm run check

# 3. Check Rust code formatting
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

# 4. Run Rust unit tests
cargo test --manifest-path src-tauri/Cargo.toml

# 5. Run Rust linter (Clippy)
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

To auto-format Rust code:
```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
```

---

## Pull Request Guidelines

1. **Branch Naming**: Use descriptive branch names like `fix/sidebar-resizer` or `feat/terminal-shortcuts`.
2. **Atomic Commits**: Keep commits focused and provide clear commit messages following conventional commits (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`).
3. **Tests**: Add unit tests for any new logic or bug fixes.
4. **Safety Invariants**:
   - Destructive Git operations (discards, resets) must always retain a recoverable backup ref (`refs/vantage/backup/*` or `.git/vantage-trash/`).
   - The code viewer must remain strictly read-only.
5. **Continuous Integration**: Ensure that all GitHub Actions CI checks pass on your PR.

---

## Reporting Issues & Requesting Features

- **Bug Reports**: Please include your operating system, Vantage version, steps to reproduce, and any relevant error logs from the terminal or webview console.
- **Feature Requests**: Describe the problem you are trying to solve and how you envision the feature working within Vantage's multi-pane and review-first architecture.
- **Security Vulnerabilities**: Please see [SECURITY.md](SECURITY.md) for how to report vulnerabilities privately.
