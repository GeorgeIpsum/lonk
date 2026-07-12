import { defineConfig } from 'vite';

// Dev loop: run `cargo run --bin lonkd` (port 8000), then `npm -w web run dev`.
// API calls and short-link routes proxy through so links are clickable.
export default defineConfig({
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8000',
      '^/[A-Za-z0-9]{7}(/(qr|status))?$': 'http://127.0.0.1:8000',
    },
  },
});
