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

## Finding an Issue

New contributors should start by finding an issue labeled **`good first issue`**. This label marks tasks scoped for learning the codebase with minimal risk.

### Issue Labels & Difficulty

- **`good first issue`**: Scoped for newcomers. Self-contained, well-documented, and low risk. Start here.
- **`help wanted`**: Contribution welcome but may require some codebase familiarity.
- **`difficulty: easy`**: Straightforward task, likely in one module or document.
- **`difficulty: medium`**: Requires understanding multiple components or APIs.
- **`difficulty: hard`**: Complex, cross-cutting, or touches contract internals.
- **`type: bug`**: Defect that needs fixing.
- **`type: feature`**: New capability or enhancement.
- **`type: docs`**: Documentation improvement.

### Assignment Etiquette

Before starting work on an issue:

1. **Check if it's assigned**: If someone is already working on it, pick a different issue.
2. **Comment to claim it**: Reply with "I'd like to work on this" or similar. This signals your intent and prevents duplicate work.
3. **Get guidance if unsure**: Ask for clarification on scope or approach in the issue comments. The maintainers will help.

### Finding Good First Issues

**GitHub filter**: [Good first issue filter](https://github.com/stellar/tikka-contracts/issues?q=is%3Aissue+is%3Aopen+label%3A"good+first+issue")

You can also filter by `good first issue` and `type:docs` to start with documentation improvements, which are lower-risk and help the community.

### Terminology Reference

Unfamiliar with a term in the issue? Check [`docs/GLOSSARY.md`](docs/GLOSSARY.md) for one-paragraph definitions with code references.

## Development Expectations

- Keep changes scoped and easy to review.
- Write tests for new behavior when possible.
- Update documentation if behavior or APIs change.
- Include the corresponding documentation update in the same PR whenever
	behavior or an API changes; mark unfinished behavior as unimplemented.

## Tests

```bash
cargo test -p raffle-factory
cargo test -p raffle-instance
```

## Error Documentation Sync

If you modify or add any variants to the `Error` enum in `contracts/raffle-instance/src/lib.rs`, regenerate `docs/ERRORS.md` before committing:

```bash
python scripts/generate_error_docs.py
```

CI will fail if `docs/ERRORS.md` is out of sync with the Rust `Error` enum.

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
