import { mount, hydrate } from 'svelte';
import './app.css';
import App from './App.svelte';
import Shot from './Shot.svelte';

// `?shot=<name>` renders a single terminal/report view at a fixed size: used to produce the
// README screenshots with agent-browser (see site/README.md).
const shot = new URLSearchParams(location.search).get('shot');
const target = document.getElementById('app');
if (shot) {
  mount(Shot, { target, props: { name: shot } });
} else if (target.hasChildNodes()) {
  // The build prerenders the page (scripts/prerender.mjs); attach to that markup.
  hydrate(App, { target });
} else {
  mount(App, { target });
}
