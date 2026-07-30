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

