import { Alerter } from './alert/alerter';
import { loadAndValidateConfig } from './config';

/**
 * Bootstrap entry point. Currently wires operational alerting (process
 * start/stop) so operators are notified when the oracle goes down. Service
 * wiring (KeyService, EventListenerService, TxSubmitterService) plugs in here.
 */
async function main(): Promise<void> {
  const config = loadAndValidateConfig();

  const alerter = new Alerter({
    webhookUrl: config.alertWebhookUrl,
    rateLimitMs: config.alertRateLimitMs,
  });

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

  let shuttingDown = false;
  const shutdown = (signal: string) => {
    if (shuttingDown) {
      return;
    }
    shuttingDown = true;
    void alerter
      .notify({
        type: 'process_stop',
        severity: 'info',
        message: `Oracle service stopped (${signal})`,
      })
      .finally(() => process.exit(0));
  };

  process.on('SIGINT', () => shutdown('SIGINT'));
  process.on('SIGTERM', () => shutdown('SIGTERM'));
}

main().catch((error) => {
  console.error(`Oracle service failed to start: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});