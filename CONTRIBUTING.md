# Contributing

Thanks for your interest in contributing to Tikka! This project targets Stellar/Soroban smart contracts and welcomes PRs for improvements, tests, and docs.

## Getting Started

1. Fork the repository and create a feature branch.
1. Make your changes with clear, focused commits.
1. Run `cargo fmt --all` to format code before committing.
1. Run tests locally before opening a PR.

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

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).
