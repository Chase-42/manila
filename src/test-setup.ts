import { randomFillSync } from 'crypto';

// jsdom does not ship a WebCrypto implementation; @tauri-apps/api/mocks requires it.
Object.defineProperty(window, 'crypto', {
	value: {
		getRandomValues: (buffer: NodeJS.ArrayBufferView) => randomFillSync(buffer as Buffer),
	},
});
