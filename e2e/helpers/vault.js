const E2E_PASSWORD = 'e2e-test-password';

/**
 * Unlock the vault at the start of an E2E session.
 *
 * The app always starts locked. This helper handles both modes:
 * - Uninitialized (first run): fills password + confirm, calls create_vault.
 * - Initialized but locked: fills password only, calls unlock_vault.
 *
 * Call once from the wdio.conf.js `before` hook so all specs start with the
 * app shell accessible.
 */
export async function ensureVaultUnlocked() {
  // Wait for init_db and vault_status to resolve before inspecting the DOM.
  await browser.pause(2000);

  const lockScreen = await browser.$('.lock-screen');
  if (!(await lockScreen.isExisting())) return;

  const passwordInput = await browser.$('input[placeholder="Master password"]');
  await passwordInput.setValue(E2E_PASSWORD);

  const confirmInput = await browser.$('input[placeholder="Confirm password"]');
  if (await confirmInput.isExisting()) {
    await confirmInput.setValue(E2E_PASSWORD);
  }

  const submitBtn = await browser.$('button[type="submit"]');
  await submitBtn.click();

  await browser.waitUntil(
    async () => !(await (await browser.$('.lock-screen')).isExisting()),
    { timeout: 10000, timeoutMsg: 'lock screen did not disappear after unlock' },
  );

  // Brief settle so the app shell is fully rendered before specs begin.
  await browser.pause(500);
}
