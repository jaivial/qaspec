import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Served from https://jaivial.github.io/qaspec/
export default defineConfig({
  base: process.env.QASPEC_SITE_BASE ?? '/qaspec/',
  plugins: [svelte()],
  build: { outDir: 'dist', emptyOutDir: true }
});
