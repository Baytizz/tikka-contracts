# Contributing

Thanks for your interest in contributing to Tikka! This project targets Stellar/Soroban smart contracts and welcomes PRs for improvements, tests, and docs.

## Getting Started

1. Fork the repository and create a feature branch.
1. Make your changes with clear, focused commits.
1. Run `cargo fmt --all` to format code before committing.
1. Run tests locally before opening a PR.

Setup problems (missing WASM target, Stellar CLI vs SDK 23 mismatch, Node 20 for `oracle/`, `stellar` vs `soroban` naming, deploy script paths) are answered in [`docs/FAQ.md`](docs/FAQ.md).

## Development Expectations

- Keep changes scoped and easy to review.
- Write tests for new behavior when possible.
- Update documentation if behavior or APIs change.

## Tests

```bash
cargo test -p raffle-factory
cargo test -p raffle-instance
```

## Markdown

Run markdownlint before opening a PR to keep documentation style consistent:

```bash
npx markdownlint-cli2 "**/*.md"
```

The configuration lives in `.markdownlint.jsonc`. Auto-fixable issues can be resolved with `npx markdownlint-cli2 --fix "**/*.md"`.

## Pull Requests

- Provide a concise summary of what changed and why.
- Link any relevant issues.
- Note any follow-up work or limitations.
- Use the PR template at `.github/PULL_REQUEST_TEMPLATE.md` to ensure all required information is included.

## Stale issues and PRs

To keep the contribution queue healthy, we run GitHub's [`actions/stale`](https://github.com/actions/stale) bot (see [`.github/workflows/stale.yml`](.github/workflows/stale.yml)):

- Issues and pull requests with no activity for **21 days** are marked `stale` with a friendly reminder.
- If there is still no activity for **7 more days**, they are closed automatically.
- Items labeled `critical` or assigned to a milestone are exempt.

If your issue or PR is marked stale and you are still working on it, leave a comment or push an update and we will gladly keep it open.

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).
