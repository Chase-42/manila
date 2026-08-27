import path from 'path';
import { spawnSync } from 'child_process';
import { fileURLToPath } from 'url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const root = path.resolve(__dirname, '..');

export const config = {
  runner: 'local',
  specs: ['./specs/**/*.spec.js'],
  maxInstances: 1,

  services: [
    [
      '@wdio/tauri-service',
      {
        // embedded provider: no external tauri-driver; works on macOS natively
        driverProvider: 'embedded',
        appBinaryPath: path.resolve(root, 'src-tauri/target/debug/manila'),
      },
    ],
  ],

  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        application: path.resolve(root, 'src-tauri/target/debug/manila'),
      },
    },
  ],

  logLevel: 'info',
  bail: 0,
  waitforTimeout: 10000,
  connectionRetryTimeout: 90000,
  connectionRetryCount: 3,

  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },

  reporters: ['spec'],

  // Build the app before any test session starts.
  // --no-bundle skips installer creation but still produces the debug binary.
  onPrepare: () => {
    console.log('Building Tauri debug binary...');
    const result = spawnSync(
      'pnpm',
      ['tauri', 'build', '--debug', '--no-bundle'],
      { cwd: root, stdio: 'inherit', shell: true }
    );
    if (result.status !== 0) {
      throw new Error(`tauri build failed with exit code ${result.status}`);
    }
  },
};
