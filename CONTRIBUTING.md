# Contributing

Thanks for your interest in contributing to Tikka! This project targets Stellar/Soroban smart contracts and welcomes PRs for improvements, tests, and docs.

## Getting Started

1. Fork the repository and create a feature branch.
2. Make your changes with clear, focused commits.
3. Run `cargo fmt --all` to format code before committing.
4. Run tests locally before opening a PR.
5. Install the recommended VS Code extensions when prompted and keep format-on-save enabled.
6. Install the local hooks with `pip install pre-commit && pre-commit install`.

Setup problems (missing WASM target, Stellar CLI vs SDK 23 mismatch, Node 20 for `oracle/`, `stellar` vs `soroban` naming, deploy script paths) are answered in [`docs/FAQ.md`](docs/FAQ.md).

## Development Expectations

-   Keep changes scoped and easy to review.
-   Write tests for new behavior when possible.
-   Update documentation if behavior or APIs change.

## Tests

```bash
cargo test -p raffle-factory
cargo test -p raffle-instance
```

## Pull Requests

-   Provide a concise summary of what changed and why.
-   Link any relevant issues.
-   Note any follow-up work or limitations.
-   Use the PR template at `.github/PULL_REQUEST_TEMPLATE.md` to ensure all required information is included.

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).

