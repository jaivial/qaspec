<script>
  import Terminal from './lib/Terminal.svelte';
  import Report from './lib/Report.svelte';
  import Code from './lib/Code.svelte';
  import run from './data/run.txt?raw';
  import report from './data/report.json';
  import agentSpec from '../../tests/fixtures/app/specs/app-agent.qa.ts?raw';
  import shopToml from '../../examples/shop/qaspec.toml?raw';
  import checkoutSpec from '../../examples/shop/specs/store/checkout.qa.ts?raw';
  import ordersSpec from '../../examples/shop/specs/backoffice/orders.qa.ts?raw';

  const base = import.meta.env.BASE_URL;
  const version = '0.1.0';
  const repo = 'https://github.com/jaivial/qaspec';

  const features = [
    ['Intent, not locators', 'Specs are goals and expectations. No selectors, waits or helpers. Anything outside the DSL is rejected with file:line:col.'],
    ['Sees what a test sees', 'Console errors, uncaught exceptions, every request and status, localStorage and any JS expression, checked per step.'],
    ['Many checks, one file', 'A suite is an ordered journey. Steps share browser state, and a failure skips only the steps that depend on it.'],
    ['One browser per run', 'A Chromium session costs ~1.5 GB. qaspec uses a single agent-browser session with one tab per project and reuses it across suites.'],
    ['Sign in once', 'Identities sign in with a login spec or by themselves. State is saved (mode 600) and reused across runs until valid_if fails.'],
    ['Secrets stay secret', "The model only sees secret names. Values go through agent-browser's stdin, never argv, and are redacted from every log and report."]
  ];

  const expectations = [
    ["expect('…')", "Judged against the screen and this step's signals, with quoted evidence", '1 call'],
    ['expect.console.noErrors()', 'No console.error or uncaught error during the step', '—'],
    ["expect.network('POST /api/**').status(201)", 'Requests made during the step', '—'],
    ['expect.network.noFailures()', 'No response ≥ 400', '—'],
    ["expect.state('window.x').equals(1)", 'Any JS expression in the page', '—'],
    ["expect.url().toContain('/x')", 'Current URL', '—'],
    ["expect.visible('Welcome')", 'Text on the page', '—'],
    ["expect.storage.local('k').equals('v')", 'localStorage', '—']
  ];

  let tab = $state('checkout');
  const shopFiles = { checkout: ['specs/store/checkout.qa.ts', checkoutSpec, 'ts'], orders: ['specs/backoffice/orders.qa.ts', ordersSpec, 'ts'], toml: ['qaspec.toml', shopToml, 'toml'] };

  let copied = $state(false);
  const install = 'cargo install --git https://github.com/jaivial/qaspec --tag v0.1.0';
  async function copy() {
    await navigator.clipboard.writeText(install);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<header class="top">
  <nav class="wrap">
    <a class="logo" href="#top">qa<span>spec</span></a>
    <div class="links">
      <a href="#demo">Demo</a><a href="#config">Config</a><a href="#reference">Reference</a>
      <a href="{repo}/blob/main/CHANGELOG.md">Changelog</a><a class="gh" href={repo}>GitHub</a>
    </div>
  </nav>
</header>

<main id="top">
  <section class="hero wrap">
    <div>
      <p class="pill">v{version} · Rust · built on agent-browser</p>
      <h1>An agent that <em>clicks like a person</em> and inspects like a test.</h1>
      <p class="lead">Write goals and expectations. An LLM agent drives a real browser through
        <a href="https://github.com/vercel-labs/agent-browser">agent-browser</a> and checks what's on screen,
        <strong>and</strong> the console, uncaught errors, network and app state.</p>
      <div class="cta">
        <a class="btn primary" href="#install">Get started</a>
        <a class="btn" href="{repo}/releases/latest">Download v{version}</a>
      </div>
    </div>
    <Code code={agentSpec} title="specs/todos.qa.ts — parsed, never executed" />
  </section>

  <section class="wrap" id="demo">
    <h2>A real run</h2>
    <p class="sub">Output and report from the bundled fixture app: an agent-driven journey, deterministic checks, a page that throws and returns a 500, and a second project that reuses a value captured in the first. One browser session for all of it.</p>
    <div class="demo">
      <Terminal title="qaspec run" text={'$ qaspec run\n' + run} />
      <div>
        <p class="hint">The same run as <code>.qaspec/report.json</code>. Click a step to see its checks, evidence and the agent's actions.</p>
        <Report {report} />
      </div>
    </div>
    <figure class="shot">
      <img src="{base}screenshots/agent-view.png" alt="The fixture app's Todos page with agent-browser's numbered labels on every interactive element" width="1100" height="640" loading="lazy" />
      <figcaption>What the agent works with: agent-browser numbers every control (<code>[2]</code> = ref <code>e2</code>). The agent reads the snapshot, picks refs and acts, while qaspec records console, errors and network for the step.</figcaption>
    </figure>
  </section>

  <section class="wrap">
    <h2>Why qaspec</h2>
    <p class="sub">Classic E2E tests are slow to write and blind to what they don't assert. Agentic frameworks hide the console and network from the model. qaspec gives the agent both.</p>
    <div class="grid">
      {#each features as [t, d]}<div class="card"><h3>{t}</h3><p>{d}</p></div>{/each}
    </div>
  </section>

  <section class="wrap" id="install">
    <h2>Install</h2>
    <p class="sub">A single Rust binary. The only runtime dependency is agent-browser.</p>
    <div class="install">
      <Terminal title="shell" text={`$ npm i -g agent-browser && agent-browser install\n$ ${install}\n$ qaspec init      # qaspec.toml + specs/home.qa.ts\n$ qaspec check     # validate specs, config, secrets — no browser\n$ qaspec run       # everything, in one browser session`} />
      <button class="btn copy" onclick={copy}>{copied ? 'Copied ✓' : 'Copy install command'}</button>
    </div>
  </section>

  <section class="wrap" id="config">
    <h2>Projects, environments, credentials</h2>
    <p class="sub">Everything a spec needs lives in <code>qaspec.toml</code>. Specs only name things. The <code>examples/shop</code> project has two apps that depend on each other:</p>
    <div class="tabs" role="tablist">
      {#each Object.entries(shopFiles) as [k, [label]]}
        <button role="tab" aria-selected={tab === k} class:active={tab === k} onclick={() => (tab = k)}>{label}</button>
      {/each}
    </div>
    {#key tab}<Code code={shopFiles[tab][1]} lang={shopFiles[tab][2]} />{/key}
    <table>
      <thead><tr><th>Need</th><th>How</th></tr></thead>
      <tbody>
        <tr><td>Switch environment</td><td><code>--env staging</code> or <code>QASPEC_ENV</code></td></tr>
        <tr><td>Override a value</td><td><code>--set params.product=x</code>, <code>--set projects.store.base_url=…</code></td></tr>
        <tr><td>A project is down</td><td><code>health</code> fails → its suites and dependents are <code>blocked</code>, not failed</td></tr>
        <tr><td>Cross-app journey</td><td><code>step('…', {'{'} project: 'store' {'}'})</code> switches to that project's tab, which keeps its state</td></tr>
        <tr><td>Pass data on</td><td><code>capture('orderNumber', …)</code> → <code>{'${orderNumber}'}</code> in later steps and files</td></tr>
        <tr><td>Two users, one app</td><td>Suites are grouped by identity; switching saves and restores sign-in state, with no new browser</td></tr>
      </tbody>
    </table>
  </section>

  <section class="wrap" id="reference">
    <h2>Expectations</h2>
    <p class="sub">Deterministic checks run first and cost nothing. The LLM judge runs only if they pass.</p>
    <table>
      <thead><tr><th>Call</th><th>Checks</th><th>Model</th></tr></thead>
      <tbody>{#each expectations as [c, d, m]}<tr><td><code>{c}</code></td><td>{d}</td><td>{m}</td></tr>{/each}</tbody>
    </table>
    <p class="sub foot">Full reference in the <a href="{repo}#spec-reference">README</a>. Exit codes: 0 passed · 1 failed · 2 blocked · 3 config error. JSON and JUnit reports.</p>
  </section>
</main>

<footer><div class="wrap">qaspec v{version} · MIT · <a href={repo}>github.com/jaivial/qaspec</a> · built on <a href="https://github.com/vercel-labs/agent-browser">agent-browser</a> · site made with Svelte</div></footer>

<style>
  .top { border-bottom: 1px solid var(--line); position: sticky; top: 0; background: rgba(13,17,23,.85); backdrop-filter: blur(8px); z-index: 10; }
  nav { display: flex; align-items: center; justify-content: space-between; height: 60px; }
  .logo { font: 700 20px var(--mono); color: var(--text); } .logo span { color: var(--accent); } .logo:hover { text-decoration: none; }
  .links a { margin-left: 22px; color: var(--muted); } .links a:hover { color: var(--text); text-decoration: none; }
  .links .gh { color: var(--text); border: 1px solid var(--line); padding: 4px 12px; border-radius: 6px; }
  .hero { padding: 64px 24px 40px; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.05fr); gap: 44px; align-items: center; }
  .pill { display: inline-block; font: 12px var(--mono); color: var(--accent); border: 1px solid rgba(63,185,80,.4); border-radius: 999px; padding: 2px 12px; margin: 0 0 16px; }
  h1 { font-size: 46px; line-height: 1.08; margin: 0 0 18px; letter-spacing: -.02em; } h1 em { font-style: normal; color: var(--accent); }
  .lead { color: var(--muted); font-size: 18px; margin: 0 0 28px; }
  .btn { display: inline-block; padding: 10px 18px; border-radius: 6px; font-weight: 600; margin-right: 10px; border: 1px solid var(--line); color: var(--text); background: transparent; font-size: 15px; cursor: pointer; }
  .btn.primary { background: var(--accent); color: #04120a; border-color: var(--accent); }
  .btn:hover { text-decoration: none; filter: brightness(1.12); }
  section { padding: 48px 24px; border-top: 1px solid var(--line); }
  .hero { border-top: 0; }
  h2 { font-size: 28px; margin: 0 0 8px; } .sub { color: var(--muted); margin: 0 0 26px; max-width: 820px; } .foot { margin-top: 18px; }
  .demo { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 22px; align-items: start; }
  .hero > :global(*), .demo > :global(*) { min-width: 0; }
  .hint { color: var(--muted); font-size: 14px; margin: 0 0 10px; }
  .shot { margin: 30px 0 0; } .shot img { width: 100%; height: auto; border-radius: 10px; border: 1px solid var(--line); display: block; }
  figcaption { color: var(--muted); font-size: 14px; margin-top: 10px; }
  .grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 18px; }
  .card { background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 20px; }
  .card h3 { margin: 0 0 8px; font-size: 17px; } .card p { margin: 0; color: var(--muted); font-size: 15px; }
  .install { max-width: 760px; } .copy { margin-top: 14px; }
  .tabs { display: flex; gap: 4px; margin-bottom: -1px; overflow-x: auto; }
  .tabs button { flex: none; }
  .tabs button { font: 13px var(--mono); color: var(--muted); background: transparent; border: 1px solid transparent; border-bottom: 0; padding: 8px 14px; border-radius: 8px 8px 0 0; cursor: pointer; }
  .tabs button.active { color: var(--text); background: var(--panel); border-color: var(--line); }
  table { width: 100%; border-collapse: collapse; font-size: 15px; margin-top: 26px; }
  th, td { text-align: left; padding: 10px 12px; border-bottom: 1px solid var(--line); vertical-align: top; }
  th { color: var(--muted); font-weight: 600; }
  footer { border-top: 1px solid var(--line); padding: 28px 0 40px; color: var(--muted); font-size: 14px; }
  @media (max-width: 900px) {
    .hero, .demo { grid-template-columns: minmax(0, 1fr); }
    table { display: block; overflow-x: auto; } .grid { grid-template-columns: 1fr; } h1 { font-size: 34px; }
    .links a:not(.gh) { display: none; }
  }
</style>
