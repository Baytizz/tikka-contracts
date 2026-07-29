/**
 * proof-message.ts
 *
 * Builds the exact byte buffer that the oracle signs and the on-chain verifier
 * checks via `env.crypto().ed25519_verify`.
 *
 * On-chain encoding (contracts/raffle-instance/src/lib.rs, `provide_randomness`):
 *
 *   let message = Bytes::from_array(&env, &random_seed.to_be_bytes());
 *   env.crypto().ed25519_verify(&public_key, &message, &proof);
 *
 * The message is therefore exactly 8 bytes: the u64 `random_seed` encoded as
 * big-endian (most-significant byte first).  Any deviation — wrong byte order,
 * extra context bytes, truncation — will cause `ed25519_verify` to reject the
 * proof and randomness delivery will fail silently.
 *
 * This module is the single source of truth for the TS side of that encoding.
 * Both the oracle signing path and the golden-vector tests import from here so
 * a drift in this file is caught by tests before it reaches any environment.
 */

/**
 * Encodes `randomSeed` (a 64-bit unsigned integer represented as a `bigint`)
 * into the 8-byte big-endian buffer that the on-chain verifier expects.
 *
 * @param randomSeed - The VRF output seed.  Must be in [0, 2^64).
 * @returns An 8-byte `Buffer` suitable for passing to `Keypair.sign()`.
 * @throws {RangeError} if `randomSeed` is negative or >= 2^64.
 */
export function buildProofMessage(randomSeed: bigint): Buffer {
  if (randomSeed < 0n) {
    throw new RangeError(`randomSeed must be non-negative, got ${randomSeed}`);
  }
  if (randomSeed >= 2n ** 64n) {
    throw new RangeError(
      `randomSeed must be < 2^64 (${2n ** 64n}), got ${randomSeed}`,
    );
  }

  // Mirror the Rust encoding: random_seed.to_be_bytes() — 8 bytes, big-endian.
  const buf = Buffer.allocUnsafe(8);
  // Node's Buffer.writeBigUInt64BE writes the full 8-byte big-endian u64.
  buf.writeBigUInt64BE(randomSeed);
  return buf;
}
