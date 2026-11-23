# First Contribution Walkthrough

This walkthrough takes you from fork to merged PR.

## 1) Fork and Clone

Fork the repository on GitHub, then clone your fork:

```bash
git clone https://github.com/YOUR_USERNAME/RetinaX.git
cd RetinaX
git remote add upstream https://github.com/RetinaX-Org/RetinaX.git
```

## 2) Setup and Verify

Run the setup script and verify both Rust and website tests pass:

```bash
./setup.sh
make test
cd website && npm ci && npm test && npm run lint && npm run format:check
```

## 3) Find a Good First Issue

Use the [good first issue guide](good-first-issues.md) to check labels, assignees, scope, and acceptance criteria before you start. Confirm the issue is assigned to you, then comment with your plan if the issue asks for confirmation.

## 4) Create a Feature Branch

Branch naming conventions:

- `feature/<issue-id>-short-title`
- `fix/<issue-id>-short-title`
- `docs/<issue-id>-short-title`

```bash
git checkout -b feature/123-add-vision-export
```

## 5) Make Changes with Code Style

- Format Rust: `cargo fmt`
- Lint Rust: `cargo clippy --all-targets --all-features -- -D warnings`
- Format website JavaScript: `cd website && npm run format`
- Keep changes focused and well-scoped.

## 6) Write Tests

Add or update tests that cover your change. Rust changes use `cargo test --all`; website and documentation checks use `node --test` under `website/`.

## 7) Run the Local CI Pipeline

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo deny check
cargo audit
make test-js
cd website
npm run format:check
npm run lint
npm test
```

Documentation PRs should also check the Markdown link config with Lychee where available.

## 8) Commit with Conventional Commits

```bash
git commit -m "feat: add export support for records (closes #123)"
```

## 9) Push and Open a PR

```bash
git push origin feature/123-add-vision-export
```

Fill out the PR template, link the issue with `Closes #123`, and include testing output.

## 10) Respond to Review

- Address requested changes quickly.
- Push follow-up commits to the same branch.
- Keep the PR focused so maintainers can review it before the issue due date.

## 11) Celebrate

Once merged, update any follow-up docs or issues if needed.
