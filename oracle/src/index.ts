import { GracefulShutdown } from './shutdown/graceful-shutdown';
import { RequestQueue } from './queue/request-queue';
import { FileLedgerCheckpointStore } from './listener/ledger-checkpoint';
import { EventListenerService } from './listener/event-listener.service';
import { KeyService } from './keys/key.service';
import { VrfService } from './vrf/vrf.service';
import { TxSubmitterService } from './tx/tx-submitter.service';
import { DeduplicationStore } from './deduplication/deduplication.store';
import { loadAndValidateConfig } from './config';

async function main() {
  const config = loadAndValidateConfig();

  const keyService = new KeyService();
  await keyService.initialize();

  const queue = new RequestQueue();
  const checkpointStore = new FileLedgerCheckpointStore('./data/ledger-checkpoint.json');
  const dedupStore = new DeduplicationStore();

  const vrfService = new VrfService(keyService);
  const submitter = new TxSubmitterService(keyService, config.rpcUrl);

  const listener = new EventListenerService(
    queue,
    keyService.getPublicKey(),
    checkpointStore,
    { pollIntervalMs: config.pollIntervalMs },
  );

  const shutdown = new GracefulShutdown(queue, checkpointStore, {
    drainTimeoutMs: Number(process.env.DRAIN_TIMEOUT_MS ?? 30_000),
    processJob: async (job) => {
      if (dedupStore.isDuplicate(job.requestId, job.raffleContract)) {
        return false;
      }
      const randomSeed = BigInt(Date.now());
      const proof = vrfService.signRandomnessProof(
        job.raffleContract,
        job.requestId,
        randomSeed,
      );
      await submitter.submitProvideRandomness({
        raffleContract: job.raffleContract,
        randomSeed: proof.randomSeed,
        publicKey: proof.publicKey,
        proof: proof.proof,
        requestId: proof.requestId,
      });
      return true;
    },
  });

  // Register SIGTERM/SIGINT before startListening so no signal is missed.
  shutdown.register(() => listener.stopListening());

  await listener.initialize();
  console.log('Oracle service started. Listening for RandomnessRequested events.');
  await listener.startListening([config.factoryContractId]);
}

main().catch((err) => {
  console.error('Fatal oracle error:', err);
  process.exit(1);
});
