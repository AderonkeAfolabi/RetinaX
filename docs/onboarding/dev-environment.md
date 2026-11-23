# Development Environment Deep Dive

This guide covers Linux, macOS, and Windows (WSL2). It assumes you are working in the repository root.

## Supported Platforms

- Linux (Ubuntu 22.04+ recommended)
- macOS (Apple Silicon or Intel)
- Windows using WSL2 (Ubuntu recommended)

### Windows (WSL2) Notes

- Install WSL2 and Ubuntu, then do all work inside the WSL2 filesystem.
- Avoid building inside `/mnt/c` to prevent file watcher and permission issues.
- Use Windows Git only for GitHub Desktop if needed, but run builds in WSL2.

## Rust Toolchain

`Cargo.toml` declares Rust 1.78 as the workspace minimum. CI uses stable Rust, and the offchain server documents a newer stable minimum in its own README. Unless you are reproducing an old bug, use the stable toolchain.

```bash
rustup toolchain install stable
rustup default stable
rustup component add rustfmt clippy rust-src
rustc --version
cargo --version
```

Soroban contracts compile to WASM for on-chain deployment:

```bash
rustup target add wasm32-unknown-unknown
```

Common failure signs:

- `error: cannot find target wasm32-unknown-unknown`
- `unknown target 'wasm32-unknown-unknown'`

## Soroban CLI

Install the Soroban CLI without pinning a stale version. Use a release compatible with the workspace `soroban-sdk`.

```bash
cargo install --locked soroban-cli
soroban --version
```

If a contract build reports a CLI/SDK mismatch, install a newer Soroban CLI and confirm the version against the project SDK in `Cargo.toml`.

## Node.js and Website Tooling

The website has no build step, but its JavaScript tests and quality checks run with Node.js 20+ and npm.

```bash
node --version
npm --version
cd website
npm ci
npm test
npm run lint
npm run format:check
```

`make test` and `make test-js` also run the website tests from the repository root.

## Additional Cargo Tools

```bash
cargo install cargo-fuzz
cargo install cargo-deny
cargo install cargo-audit
```

## Pre-Commit Hooks

This repo ships a `.pre-commit-config.yaml`. Install and enable hooks:

```bash
pipx install pre-commit
pre-commit install
pre-commit run --all-files
```

## IDE Configuration

### VS Code

Recommended extensions:

- rust-lang.rust-analyzer
- vadimcn.vscode-lldb
- serayuzgur.crates
- tamasfe.even-better-toml
- github.vscode-github-actions

Recommended settings:

- Set `rust-analyzer.checkOnSave.command` to `clippy`.
- Enable `editor.formatOnSave`.
- Use `rust-analyzer.cargo.features` if you need feature flags.
- Set `files.watcherExclude` for `**/target/**` to reduce file watcher load.

### IntelliJ / CLion

- Install the Rust plugin.
- Enable `Run rustfmt on Save`.
- Enable Clippy inspections for Rust.
- Configure a stable Rust toolchain.

## Offchain Server with Docker

The repository has no root Dockerfile. Docker is used by the standalone offchain server.

```bash
cd offchain-server
cp .env.example .env
docker compose up -d postgres
cargo run
curl http://127.0.0.1:8080/health
```

See `offchain-server/README.md` for database migrations, API routes, and test commands.

## Troubleshooting FAQ

1. `error: could not find Cargo.toml`
   - Ensure you are in the repo root before running cargo commands.
2. `rustc: unknown target wasm32-unknown-unknown`
   - Run `rustup target add wasm32-unknown-unknown`.
3. `linker 'cc' not found` on Linux
   - Install build tools: `sudo apt-get install build-essential`.
4. `ld: library not found for -lssl` on macOS
   - Install OpenSSL via Homebrew and set `PKG_CONFIG_PATH`.
5. Soroban CLI version mismatch
   - Install a current CLI with `cargo install --locked soroban-cli`.
6. `soroban: command not found`
   - Ensure `~/.cargo/bin` is in your PATH and restart your shell.
7. `npm: command not found` or failing website tests
   - Install Node.js 20+, then run `cd website && npm ci`.
8. `error: denied by policy` during `cargo deny check`
   - Update your advisory DB: `cargo deny fetch`.
9. `error: unable to get local issuer certificate`
   - Check corporate proxy settings or set `CARGO_HTTP_CAINFO`.
10. WSL2 file watcher slowness
    - Move the repo into the Linux filesystem and rerun.
11. Root Dockerfile not found
    - Use `offchain-server/Dockerfile` through `docker compose` from `offchain-server/`.
