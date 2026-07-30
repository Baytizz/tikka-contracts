import { loadAndValidateConfig } from './config';
import { KeyService } from './keys/key.service';
import { FileLedgerCheckpointStore } from './listener/ledger-checkpoint';
import { RequestQueue } from './queue/request-queue';
import { EventListenerService } from './listener/event-listener.service';
import { DeduplicationStore } from './deduplication/deduplication.store';
import { VrfService } from './vrf/vrf.service';
import { TxSubmitterService } from './tx/tx-submitter.service';
import http from 'node:http';
import crypto from 'node:crypto';

async function main(): Promise<void> {
  const cfg = loadAndValidateConfig();

  console.log('Starting oracle with factory:', cfg.factoryContractId);

  const keyService = new KeyService();
  await keyService.initialize();

  const checkpoint = new FileLedgerCheckpointStore('data/checkpoint.json');
  const queue = new RequestQueue();
  const dedup = new DeduplicationStore();

  const listener = new EventListenerService(queue, keyService.getPublicKey(), checkpoint, {
    pollIntervalMs: cfg.pollIntervalMs,
    rpcUrl: cfg.rpcUrl,
  });

  await listener.initialize();

  // Start listening for events from the factory contract
  listener.startListening([cfg.factoryContractId]).catch((err) => {
    console.error('Event listener failed:', err);
  });

  const vrf = new VrfService(keyService);
  const submitter = new TxSubmitterService(keyService, cfg.rpcUrl);

  // Processing loop: drain queue and submit proofs
  setInterval(
    async () => {
      const jobs = queue.drain();
      for (const job of jobs) {
        try {
          if (dedup.hasSeen(job.requestId, job.raffleContract)) {
            continue;
          }

          const randomSeed = BigInt('0x' + crypto.randomBytes(8).toString('hex'));
          const proof = vrf.signRandomnessProof(job.raffleContract, job.requestId, randomSeed);

          await submitter.submitProvideRandomness({
            raffleContract: job.raffleContract,
            randomSeed: proof.randomSeed,
            publicKey: proof.publicKey,
            proof: proof.proof,
            requestId: proof.requestId,
          });

          dedup.markSeen(job.requestId, job.raffleContract);
        } catch (err) {
          console.error('Failed to process job', job, err);
        }
      }
    },
    Math.max(1000, cfg.pollIntervalMs)
  );

  // Simple health endpoint for Docker HEALTHCHECK
  http
    .createServer((req, res) => {
      if (req.url === '/health') {
        res.writeHead(200, { 'Content-Type': 'text/plain' });
        res.end('ok');
        return;
      }
      res.writeHead(404);
      res.end();
    })
    .listen(3000, () => console.log('Health endpoint listening on :3000'));

  // Graceful shutdown
  function shutdown() {
    console.log('Shutting down...');
    listener.stopListening();
    keyService.shutdown();
    process.exit(0);
  }

  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
}

main().catch((err) => {
  console.error('Boot failed:', err);
  process.exit(1);
});
