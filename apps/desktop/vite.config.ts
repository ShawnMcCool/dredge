import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  // Tauri dev: fixed port, don't clear the terminal over cargo output.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
})
