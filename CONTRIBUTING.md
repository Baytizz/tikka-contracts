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

## Error Code Policy

Error codes are defined in `#[contracterror]` enums and are part of the on-chain ABI.
Once a code is assigned it is **never reused or reassigned**, even if the variant is
later deprecated.  New errors must follow the reserved ranges:

| Range     | Owner            | Purpose                                      |
| --------- | ---------------- | -------------------------------------------- |
| 1 – 99    | Shared           | Conditions used by both instance and factory  |
| 100 – 199 | Instance-only    | New instance-specific errors                 |
| 200 – 299 | Factory-only     | New factory-specific errors                  |

If a new shared condition is needed, add it to `ProtocolError` in
`contracts/raffle-shared/src/errors.rs` with a code in the 1–99 range, then update
both `raffle-instance/src/lib.rs` and `raffle-factory/src/lib.rs` to use it.

Run `python scripts/check_error_codes.py` before submitting a PR to verify no
duplicate discriminants exist.

## Pull Requests

- Provide a concise summary of what changed and why.
- Link any relevant issues.
- Note any follow-up work or limitations.
- Use the PR template at `.github/PULL_REQUEST_TEMPLATE.md` to ensure all required information is included.

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).
