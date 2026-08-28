# Oracle Runbook

Operational guide for the Tikka randomness oracle service.

## Health and Metrics Endpoints

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Liveness probe — returns `{"status":"ok"}` |
| `GET /metrics` | Prometheus text exposition format |

Default port: `9090` (override with `HEALTH_PORT`).

## Metrics Reference

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `oracle_requests_observed_total` | Counter | `raffle` | `RandomnessRequested` events enqueued for this oracle |
| `oracle_request_latency_seconds` | Histogram | — | Wall time from event observation to confirmed on-chain submission |
| `oracle_submissions_total` | Counter | `outcome` | Submission results: `success`, `retry`, or `fatal` |
| `oracle_queue_depth` | Gauge | — | Current number of pending randomness jobs |
| `oracle_queue_oldest_age_seconds` | Gauge | — | Age of the oldest queued job in seconds |
| `oracle_dead_letter_total` | Counter | — | Jobs permanently failed after exhausting retries |
| `oracle_listener_ledger_lag` | Gauge | — | Ledgers between network tip and last processed checkpoint |
| `oracle_rpc_errors_total` | Counter | `kind` | RPC errors by phase: `poll`, `simulate`, `send` |
| `oracle_fees_spent_stroops_total` | Counter | — | Cumulative transaction fees paid for submissions |

## Suggested Alert Rules

These mirror the thresholds in `.env.example` and the existing webhook alerter.

### Queue depth

```yaml
- alert: OracleQueueDepthHigh
  expr: oracle_queue_depth > 10
  for: 2m
  labels:
    severity: warning
  annotations:
    summary: Oracle request queue depth exceeds limit
```

Env: `ALERT_QUEUE_DEPTH_LIMIT=10`

### Queue age

```yaml
- alert: OracleQueueAgeHigh
  expr: oracle_queue_oldest_age_seconds > 300
  for: 1m
  labels:
    severity: warning
  annotations:
    summary: Oldest queued randomness request is stale
```

Env: `ALERT_QUEUE_AGE_LIMIT_MS=300000` (300 seconds)

### RPC unreachable

```yaml
- alert: OracleRpcUnreachable
  expr: increase(oracle_rpc_errors_total{kind="poll"}[5m]) >= 3
  for: 1m
  labels:
    severity: critical
  annotations:
    summary: Oracle cannot reach Soroban RPC
```

Env: `ALERT_RPC_UNREACHABLE_THRESHOLD=3`

### Submission failures

```yaml
- alert: OracleSubmissionFailures
  expr: increase(oracle_submissions_total{outcome="fatal"}[10m]) > 0
  for: 0m
  labels:
    severity: critical
  annotations:
    summary: Oracle failed to submit provide_randomness
```

Env: `ALERT_FAILURE_THRESHOLD=3` (consecutive failures before webhook alert)

### Listener lag

```yaml
- alert: OracleListenerLag
  expr: oracle_listener_ledger_lag > 50
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: Oracle event listener is falling behind chain tip
```

### Fee burn rate

```yaml
- alert: OracleFeeBurnHigh
  expr: rate(oracle_fees_spent_stroops_total[1h]) > 1000000
  for: 15m
  labels:
    severity: info
  annotations:
    summary: Oracle transaction fee spend rate is elevated
```

## Crash-safety and Deduplication

### Failure windows

1. Crash before checkpointing ledger: events may be re-processed after restart; deduplication must prevent double-submission.
2. Crash after submission but before persisting dedup record: submission may succeed on-chain but the off-chain store not reflect it (risk of duplicate submission after restart).
3. Crash between enqueue and submission: a job may be lost if it was only in-memory and not checkpointed.

### Current design

- `ledger-checkpoint` persists the last processed ledger to `data/checkpoint.json`.
- `DeduplicationStore` persists seen requests to `data/seen-requests.json` and provides `hasSeen()` and `markSeen()`.
- The service marks a request as seen only after a successful on-chain confirmation (`markSeen`), to avoid losing requests that haven't been submitted yet.

### Tradeoffs and mitigation

- Marking deduplication *after* successful submission avoids false-positive filtering (lost requests), but introduces a tiny window where a crash after on-chain success but before `markSeen()` could lead to a duplicate submission. To reduce that risk:
  - The dedup store is written synchronously to disk on each `markSeen()` call.
  - The ledger checkpoint is advanced only after processing the events; the listener's checkpointing ensures we don't skip events silently.

### Recommendations (next steps)

- For stronger guarantees, consider a write-ahead approach: persist an "in-flight" marker before submission and atomically flip to "completed" after confirmation.
- Add restart-simulation tests covering: crash before checkpoint, crash after submission before dedup persist, and crash during submission.

## TypeScript Strict Mode

`oracle/tsconfig.json` enables `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, and related flags. `skipLibCheck: true` is retained as a pragmatic choice — Stellar SDK type errors in `node_modules` are not surfaced. Run `npm run typecheck` locally before pushing.
