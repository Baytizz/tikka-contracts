import { Alerter } from './alert/alerter';
import { loadAndValidateConfig } from './config';
import { startHealthServer } from './health/health.server';
import { OraclePipeline } from './pipeline';

/**
 * Bootstrap entry point. Wires the full oracle pipeline and exposes /health and
 * /metrics for observability.
 */
async function main(): Promise<void> {
  const config = loadAndValidateConfig();

  const alerter = new Alerter({
    webhookUrl: config.alertWebhookUrl,
    rateLimitMs: config.alertRateLimitMs,
  });

  startHealthServer();

  if (!alerter.enabled) {
    console.warn('ALERT_WEBHOOK_URL is not set; operational alerts are disabled.');
  } else {
    await alerter.notify({
      type: 'process_start',
      severity: 'info',
      message: `Oracle service started (poll interval ${config.pollIntervalMs}ms)`,
      details: { rpcUrl: config.rpcUrl, pollIntervalMs: config.pollIntervalMs },
    });
  }

  const pipeline = new OraclePipeline({
    config,
    alerter,
  });

  await pipeline.start([config.factoryContractId]);
}

main().catch((error) => {
  console.error(`Oracle service failed to start: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
