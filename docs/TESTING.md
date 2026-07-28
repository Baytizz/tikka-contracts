# Testing

This project uses Soroban testutils to keep an eye on resource growth as the
raffle instance evolves.

## Resource Benchmarks

The `raffle-instance` test suite measures CPU and memory with
`env.cost_estimate().budget()` around the key entrypoints below:

| Entry point | Realistic size | CPU ceiling | Memory ceiling |
| --- | ---: | ---: | ---: |
| `buy_tickets` | 1,000 tickets in one batch | `30_000_000` | `10 MiB` |
| `finalize_raffle` | 10,000 tickets sold | `80_000_000` | `24 MiB` |
| `get_my_tickets` | 10,000 owned tickets | `15_000_000` | `8 MiB` |

These are intentionally generous ceilings, roughly 2x the current expected
usage, so normal feature growth should keep passing while regressions fail
loudly.

## Notes

- The benchmark tests live in `contracts/raffle-instance/src/test.rs`.
- The suite currently includes:
  - `buy_tickets_cost_stays_below_ceiling_for_1k_batch`
  - `finalize_raffle_cost_stays_below_ceiling_for_10k_tickets`
  - `get_my_tickets_cost_stays_below_ceiling_for_10k_owned_tickets`
- A clean `cargo test -p raffle-instance` run is still blocked in this
  workspace by unrelated pre-existing syntax errors in the same test module.
  Once those are fixed, the table above should be refreshed with freshly
  measured numbers from the benchmark assertions.
