import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import Shot from './Shot.svelte';

// `?shot=<name>` renders a single terminal/report view at a fixed size: used to produce the
// README screenshots with agent-browser (see site/README.md).
const shot = new URLSearchParams(location.search).get('shot');
mount(shot ? Shot : App, { target: document.getElementById('app'), props: shot ? { name: shot } : {} });
