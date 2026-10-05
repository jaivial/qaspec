// Renders the app to HTML at build time and writes it into dist/index.html, so crawlers and
// link previews see the content without running JavaScript. The client hydrates it.
import { createServer } from 'vite';
import { readFile, writeFile } from 'node:fs/promises';

const server = await createServer({
  configFile: new URL('../vite.config.js', import.meta.url).pathname,
  server: { middlewareMode: true },
  appType: 'custom',
  logLevel: 'error'
});
try {
  const { renderApp } = await server.ssrLoadModule('/src/entry-server.js');
  const body = renderApp();
  const file = new URL('../dist/index.html', import.meta.url);
  const html = await readFile(file, 'utf8');
  if (!html.includes('<div id="app"></div>')) throw new Error('mount point not found in dist/index.html');
  await writeFile(file, html.replace('<div id="app"></div>', `<div id="app">${body}</div>`));
  console.log('prerendered', body.length, 'bytes');
} finally {
  await server.close();
}
