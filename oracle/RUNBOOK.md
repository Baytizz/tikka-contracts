# Crash-safety audit and deduplication

This document maps crash windows and explains the deduplication durability choices.

## Failure windows

1. Crash before checkpointing ledger: events may be re-processed after restart; deduplication must prevent double-submission.
2. Crash after submission but before persisting dedup record: submission may succeed on-chain but the off-chain store not reflect it (risk of duplicate submission after restart).
3. Crash between enqueue and submission: a job may be lost if it was only in-memory and not checkpointed.

## Current design

- `ledger-checkpoint` persists the last processed ledger to `data/checkpoint.json`.
- `DeduplicationStore` persists seen requests to `data/seen-requests.json` and provides `hasSeen()` and `markSeen()`.
- The service marks a request as seen only after a successful on-chain confirmation (`markSeen`), to avoid losing requests that haven't been submitted yet.

## Tradeoffs and mitigation

- Marking deduplication *after* successful submission avoids false-positive filtering (lost requests), but introduces a tiny window where a crash after on-chain success but before `markSeen()` could lead to a duplicate submission. To reduce that risk:
  - The dedup store is written synchronously to disk on each `markSeen()` call.
  - The ledger checkpoint is advanced only after processing the events; the listener's checkpointing ensures we don't skip events silently.

## Recommendations (next steps)

- For stronger guarantees, consider a write-ahead approach: persist an "in-flight" marker before submission and atomically flip to "completed" after confirmation.
- Add restart-simulation tests covering: crash before checkpoint, crash after submission before dedup persist, and crash during submission.

---

## Dead-letter queue

Randomness requests that can **never** succeed are moved to a persistent dead-letter store
(`data/dead-letter.json`) instead of retrying forever or being silently dropped.

### When a request is dead-lettered

| Trigger | Reason code | Typical cause |
|---|---|---|
| Fatal contract error | `fatal` | Raffle cancelled, draw already complete, no outstanding request, request ID mismatch |
| Retry cap exceeded | `retry_exhausted` | Transient RPC failures persisted past `QUEUE_MAX_ATTEMPTS` (default 5) |
| Queue age breach | `queue_age` | Oldest queued request older than `ALERT_QUEUE_AGE_LIMIT_MS` |
| Queue depth breach | `queue_depth` | Queue deeper than `ALERT_QUEUE_DEPTH_LIMIT` |

Each dead-letter entry includes the original job, error message, attempt count,
first-enqueued timestamp, dead-lettered timestamp, and reason. Every entry raises a
`dead_letter` alert (critical, not rate-limited).

### Inspection

```sh
# List dead-letter entries (JSON)
cat oracle/data/dead-letter.json | jq '.entries[] | {raffle: .job.raffleContract, requestId: .job.requestId, reason, error, attemptCount}'

# Health endpoint (queue + dead-letter depth)
curl -s http://127.0.0.1:3000/health | jq
```

Readiness returns HTTP 503 (`status: "degraded"`) when dead-letter depth ≥ 1 or queue
depth exceeds `ALERT_QUEUE_DEPTH_LIMIT`.

### Manual replay

Replay is appropriate only after the underlying condition is resolved (e.g. raffle
re-funded, new draw initiated with a matching request ID).

1. Inspect the entry and confirm the root cause is fixed.
2. Remove the entry from the dead-letter store (via admin tooling or by deleting
   the specific object from `dead-letter.json` and restarting).
3. Re-enqueue the job:

```typescript
import { DeadLetterStore } from './src/queue/dead-letter.store';
import { RequestQueue } from './src/queue/request-queue';

const store = new DeadLetterStore();
const queue = new RequestQueue({ deadLetterStore: store });

const entry = store.remove(raffleContract, requestId);
if (entry) {
  queue.requeue(entry.job);
}
```

4. Monitor `/health` until `deadLetterDepth` returns to 0.

Automated replay tests live in `src/queue/request-queue.test.ts` (`supports manual replay via requeue`).

