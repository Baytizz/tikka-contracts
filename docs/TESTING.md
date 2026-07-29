# Testing Guide

This document describes how to run tests and measure code coverage for the Tikka raffle platform.

## Table of Contents

- [Running Tests](#running-tests)
- [Code Coverage](#code-coverage)
  - [Prerequisites](#prerequisites)
  - [Running Coverage Locally](#running-coverage-locally)
  - [Understanding Coverage Reports](#understanding-coverage-reports)
  - [CI/CD Coverage](#cicd-coverage)
- [Test Structure](#test-structure)
- [Writing Tests](#writing-tests)
- [Best Practices](#best-practices)

## Running Tests

### Rust Contract Tests

Run all workspace tests:

```bash
cargo test --workspace
```

Run tests for a specific contract:

```bash
cargo test -p raffle-factory
cargo test -p raffle-instance
cargo test -p raffle-shared
```

Run tests with output:

```bash
cargo test --workspace -- --nocapture
```

Run a specific test:

```bash
cargo test test_name -- --nocapture
```

### Oracle Service Tests

Navigate to the oracle directory and run:

```bash
cd oracle
npm test
```

For watch mode during development:

```bash
npm test -- --watch
```

## Code Coverage

### Prerequisites

Install `cargo-llvm-cov` for coverage reporting:

```bash
cargo install cargo-llvm-cov
```

The tool requires `llvm-tools-preview` component:

```bash
rustup component add llvm-tools-preview
```

### Running Coverage Locally

#### Generate Coverage Report (LCOV format)

This generates a `lcov.info` file suitable for Codecov or other coverage tools:

```bash
cargo llvm-cov --workspace --lcov --output-path lcov.info
```

#### Generate HTML Coverage Report

For a human-readable HTML report:

```bash
cargo llvm-cov --workspace --html
```

The report will be generated in `target/llvm-cov/html/`. Open `target/llvm-cov/html/index.html` in your browser to view it.

#### View Coverage Summary in Terminal

For a quick summary in the terminal:

```bash
cargo llvm-cov --workspace
```

This will show coverage percentages for each file and the overall project.

#### Coverage for a Specific Package

To generate coverage for just one contract:

```bash
cargo llvm-cov -p raffle-factory --html
cargo llvm-cov -p raffle-instance --html
```

#### Advanced Options

Run coverage and open the HTML report automatically:

```bash
cargo llvm-cov --workspace --html --open
```

Generate coverage with test output:

```bash
cargo llvm-cov --workspace --html -- --nocapture
```

Exclude certain files or directories:

```bash
cargo llvm-cov --workspace --html --ignore-filename-regex 'test\.rs$'
```

### Understanding Coverage Reports

Coverage metrics include:

- **Line Coverage**: Percentage of executable lines that were executed during tests
- **Function Coverage**: Percentage of functions that were called during tests
- **Branch Coverage**: Percentage of conditional branches that were taken

#### Coverage Goals

- **Critical paths** (ticket purchase, winner selection, prize claims): Aim for >90% coverage
- **Administrative functions**: Aim for >80% coverage
- **Overall project**: Maintain >75% coverage

### CI/CD Coverage

Coverage is automatically measured on every pull request and push to master via GitHub Actions.

#### Viewing Coverage in CI

1. **Codecov Dashboard**: Visit the Codecov badge in the README or go to `https://codecov.io/gh/OWNER/tikka-contracts`
2. **GitHub Actions Artifacts**: 
   - Go to the Actions tab in GitHub
   - Select a workflow run
   - Download the `coverage-report` artifact
   - Extract and open `index.html`

#### Coverage Requirements

The CI pipeline:
- Generates coverage reports for every PR
- Uploads results to Codecov
- Creates an HTML artifact for manual review
- Does not fail builds on coverage decrease (informational only)

## Test Structure

### Contract Tests

The main test suite is in `contracts/raffle-instance/src/test.rs` (~1,800 lines), which covers:

- **Initialization tests**: Raffle creation and setup
- **Ticket purchase flows**: Single/multiple tickets, edge cases
- **Winner selection**: Internal randomness and oracle integration
- **Prize claiming**: Winner claims, refunds for cancelled raffles
- **Administrative operations**: Pausing, cancellation, parameter updates
- **Edge cases**: Boundary conditions, error paths

### Test Organization

Tests are organized by functionality:

```rust
#[cfg(test)]
mod tests {
    // Setup helpers
    fn setup_raffle() -> ... { }
    
    // Initialization tests
    #[test]
    fn test_create_raffle() { }
    
    // Ticket purchase tests
    #[test]
    fn test_buy_ticket() { }
    
    // Winner selection tests
    #[test]
    fn test_select_winner() { }
    
    // etc.
}
```

## Writing Tests

### Test Template

```rust
#[test]
fn test_feature_name() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Setup
    let contract = setup_test_contract(&env);
    
    // Execute
    let result = contract.function_to_test(&args);
    
    // Assert
    assert_eq!(result, expected_value);
}
```

### Testing Error Cases

```rust
#[test]
#[should_panic(expected = "ErrorCode::InvalidInput")]
fn test_invalid_input_fails() {
    let env = Env::default();
    let contract = setup_test_contract(&env);
    
    contract.function_with_invalid_input(&bad_args);
}
```

### Using Test Fixtures

Create reusable test fixtures in a separate module:

```rust
#[cfg(test)]
mod fixtures {
    pub fn standard_raffle_params() -> RaffleParams {
        RaffleParams {
            ticket_price: 100,
            max_tickets: 1000,
            // ...
        }
    }
}
```

## Best Practices

### General Testing Principles

1. **Test behavior, not implementation**: Focus on what the contract does, not how
2. **One assertion per test**: Keep tests focused and easy to debug
3. **Use descriptive test names**: `test_buying_ticket_after_deadline_fails` is better than `test_buy_ticket_2`
4. **Test edge cases**: Zero values, maximum values, boundary conditions
5. **Test error paths**: Ensure errors are raised when expected

### Coverage Best Practices

1. **Don't chase 100% coverage**: Focus on meaningful tests, not just hitting lines
2. **Prioritize critical paths**: Ticket purchases, prize distribution, winner selection
3. **Review uncovered code**: Use coverage reports to identify untested code paths
4. **Update tests with code changes**: Keep tests synchronized with implementation
5. **Document untested code**: If something is intentionally not tested, document why

### Performance

- Run full coverage reports only when needed (they're slower than regular tests)
- Use `cargo test` for rapid iteration during development
- Use `cargo llvm-cov` before submitting PRs or when investigating test gaps

## Continuous Improvement

### Identifying Test Gaps

Use coverage reports to find:
- Uncovered error handling paths
- Edge cases that aren't tested
- New features without tests
- Complex logic with low coverage

### Adding Tests

When adding tests for previously uncovered code:

1. Run coverage to identify the gap
2. Write a test that exercises the uncovered code
3. Run coverage again to verify the gap is closed
4. Ensure the test is meaningful (not just hitting lines)

## Resources

- [cargo-llvm-cov documentation](https://github.com/taiki-e/cargo-llvm-cov)
- [Soroban testing guide](https://soroban.stellar.org/docs/getting-started/testing)
- [Codecov documentation](https://docs.codecov.com/)

## Support

For questions about testing:
- Check existing tests in `contracts/raffle-instance/src/test.rs` for examples
- Review the [CONTRIBUTING.md](../CONTRIBUTING.md) guide
- Open an issue for testing-related questions
