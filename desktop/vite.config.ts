import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
export default defineConfig({ plugins: [react()], clearScreen: false, test: { environment: 'jsdom', setupFiles: ['./src/test-setup.ts'], restoreMocks: true }, server: { strictPort: true, port: 1420 } });
