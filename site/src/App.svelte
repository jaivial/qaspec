<script>
  import Terminal from './lib/Terminal.svelte';
  import Report from './lib/Report.svelte';
  import Code from './lib/Code.svelte';
  import Compare from './lib/Compare.svelte';
  import Figure from './lib/Figure.svelte';
  import run from './data/run.txt?raw';
  import report from './data/report.json';
  import agentSpec from '../../tests/fixtures/app/specs/app-agent.qa.ts?raw';
  import shopToml from '../../examples/shop/qaspec.toml?raw';
  import checkoutSpec from '../../examples/shop/specs/store/checkout.qa.ts?raw';
  import ordersSpec from '../../examples/shop/specs/backoffice/orders.qa.ts?raw';

  const version = '0.1.0';
  const repo = 'https://github.com/jaivial/qaspec';
  const install = 'cargo install --git https://github.com/jaivial/qaspec --tag v0.1.0';

  const nav = [['#how', 'How it works'], ['#compare', 'Compare'], ['#config', 'Config'], ['#reference', 'Reference']];

  // Columns: qaspec, scripted E2E (Playwright/Cypress), e2e by TesterArmy, agent-browser on its own.
  const columns = ['qaspec', 'Scripted E2E', 'e2e (TesterArmy)', 'agent-browser alone'];

  const writing = [
    ['What you write', 'Goals and expectations', 'Locators, waits, helpers', ['Code plus agent steps', 'agent.act / agent.assert in TypeScript'], 'Prompts to a coding agent'],
    ['Test file', ['Declarative .qa.ts', 'Parsed, never executed'], 'Executable TS/JS', 'Executable TypeScript', ['None', 'No test format']],
    ['Survives UI changes', ['yes', 'The agent finds the control again'], ['no', 'Selectors break'], ['yes', 'Plus a replay cache'], ['partial', 'If you re-prompt']],
    ['Many checks in order, shared state', ['yes', 'Steps in a suite'], ['yes', 'Inside one test'], ['partial', 'serial groups'], ['partial', 'Manual']],
    ['Repeatable in CI', ['yes', 'Exit codes, JSON, JUnit'], 'yes', 'yes', ['no', 'Interactive tool']]
  ];

  const seeing = [
    ['Judges the screen like a person', ['yes', "expect('…') with evidence"], 'no', ['yes', 'agent.assert'], ['partial', 'You read it']],
    ['Console errors per step', ['yes', 'expect.console.noErrors()'], ['partial', 'Hand-written listeners'], ['no', 'Hidden from the model'], ['partial', 'console command']],
    ['Network status per step', ['yes', "expect.network('POST /x').status(201)"], ['partial', 'waitForResponse'], ['no', 'Hidden from the model'], ['partial', 'network requests']],
    ['App state (JS, localStorage)', ['yes', 'expect.state, expect.storage'], ['yes', 'page.evaluate'], ['partial', 'Not to the model'], ['partial', 'eval']],
    ['Secrets kept from the model', ['yes', 'Typed via stdin, redacted'], ['partial', 'No model involved'], 'yes', ['no', 'Whatever you paste']]
  ];

  const running = [
    ['Browsers per run', ['1 Chromium', 'One tab per project'], ['1 context per test', 'Workers in parallel'], ['1 context per test', 'Workers in parallel'], ['1 per session', 'You manage it']],
    ['Sign in', ['Once per identity', 'Saved state reused across runs'], 'Setup project + storageState', 'Setup test + saved session', 'state save / load by hand'],
    ['Several apps that depend on each other', ['yes', 'depends_on, health, captures'], ['partial', 'Projects, by hand'], ['partial', 'Targets'], 'no'],
    ['Runtime', 'One Rust binary + agent-browser', 'Node + browsers', 'Node 22.12+ + Playwright', 'agent-browser'],
    ['Model calls per run', ['Every goal and judged check', 'No replay cache yet'], 'None', ['Fewer after the first run', 'Replay cache'], 'Every step']
  ];

  // On narrow screens the comparison shows qaspec against one tool at a time.
  let versus = $state(1);

  let tab = $state('checkout');
  const files = {
    checkout: ['store/checkout.qa.ts', checkoutSpec, 'ts'],
    orders: ['backoffice/orders.qa.ts', ordersSpec, 'ts'],
    toml: ['qaspec.toml', shopToml, 'toml']
  };

  let copied = $state(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(install);
      copied = true;
      setTimeout(() => (copied = false), 1600);
    } catch {}
  }

  let menu = $state(false);
</script>

<a class="skip" href="#main">Skip to content</a>
<header class="top">
  <nav class="wrap" aria-label="Main">
    <a class="logo" href="#top" aria-label="qaspec home">qa<span>spec</span></a>
    <div class="links" class:open={menu} id="menu">
      {#each nav as [href, label]}<a {href} onclick={() => (menu = false)}>{label}</a>{/each}
      <a href="{repo}/blob/main/CHANGELOG.md">Changelog</a>
    </div>
    <a class="gh" href={repo}>
      <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path fill="currentColor" d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>
      <span>GitHub</span>
    </a>
    <button class="burger" aria-expanded={menu} aria-controls="menu" aria-label="Menu" onclick={() => (menu = !menu)}>
      <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true"><path d={menu ? 'M5 5l10 10M15 5L5 15' : 'M3 6h14M3 10h14M3 14h14'} fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
    </button>
  </nav>
</header>

<main id="main">
  <section class="hero wrap" id="top">
    <div class="pitch">
      <h1>End-to-end tests that read like a QA checklist.</h1>
      <p class="lead">Write what should happen. An agent drives a real browser through
        <a href="https://github.com/vercel-labs/agent-browser">agent-browser</a>, judges the screen like a person,
        and checks the console, network and app state like a test.</p>
      <div class="cta">
        <a class="btn primary" href="#install">Install qaspec</a>
        <a class="btn" href="{repo}/releases/latest">Download v{version}</a>
      </div>
      <p class="facts">One Rust binary · one browser per run · MIT</p>
    </div>
    <Code code={agentSpec} title="specs/todos.qa.ts" />
  </section>

  <section class="wrap" id="how">
    <h2>What the agent does with that spec</h2>
    <p class="sub">Screenshots from the run below, against the small demo app in <code>tests/fixtures/app</code>.</p>
    <ol class="flow">
      <li>
        <Figure src="app-login-annotated" width="1920" height="1120" alt="Sign-in page with agent-browser's numbered labels on the email field, password field and Sign in button" />
        <h3><span class="n num">1</span>Sign in once</h3>
        <p>agent-browser labels every control. The agent types the username and fills the password with <code>fill_secret</code>, so it never sees it. The session is saved and the next run starts signed in.</p>
      </li>
      <li>
        <Figure src="app-todo-added" width="1920" height="1120" alt="Todo list showing the new item the agent added" />
        <h3><span class="n num">2</span>Reach the goal</h3>
        <p>For <code>goal('add a todo …')</code> the agent fills the input and presses Add. qaspec checks that <code>POST /api/todos</code> returned 201 and that no console error fired during the step.</p>
      </li>
      <li>
        <Figure src="app-broken" width="1920" height="1120" alt="Reports page that looks fine but is still loading" />
        <h3><span class="n num">3</span>Catch what looks fine</h3>
        <p>This page renders without visible problems, but its script throws and its API returns 500. A person clicking through would miss it. <code>expect.console.noErrors()</code> fails the step.</p>
      </li>
    </ol>
  </section>

  <section class="wrap" id="demo">
    <h2>The run, and its report</h2>
    <p class="sub">Real output, one browser session for three suites. Select a step to see its checks, the evidence and what the agent did.</p>
    <div class="demo">
      <Terminal title="qaspec run" text={'$ qaspec run\n' + run} />
      <Report {report} />
    </div>
  </section>

  <section class="wrap" id="compare">
    <h2>How it compares</h2>
    <p class="sub">qaspec sits between scripted tests and an agent you drive by hand. It borrows the agent loop from <a href="https://github.com/tester-army/e2e">e2e by TesterArmy</a>, gives the model the console and network that e2e hides, and leaves the browser to agent-browser.</p>
    <div class="versus" role="group" aria-label="Compare qaspec with">
      <span>qaspec vs</span>
      {#each columns.slice(1) as c, i}
        <button aria-pressed={versus === i + 1} onclick={() => (versus = i + 1)}>{c}</button>
      {/each}
    </div>
    <h3 class="tbl-h">Writing tests</h3>
    <Compare caption="Writing tests" {columns} rows={writing} {versus} />
    <h3 class="tbl-h">What a check can see</h3>
    <Compare caption="What a check can see" {columns} rows={seeing} {versus} />
    <h3 class="tbl-h">Running them</h3>
    <Compare caption="Running them" {columns} rows={running} {versus} />
    <p class="note">Based on e2e 0.16 and agent-browser 0.27 docs, and on porting a real two-app suite. Scripted E2E means Playwright or Cypress. Corrections welcome as <a href="{repo}/issues">issues</a>.</p>
  </section>

  <section class="wrap" id="install">
    <h2>Install</h2>
    <p class="sub">The only runtime dependency is agent-browser. Prebuilt binaries for Linux and macOS are on the <a href="{repo}/releases/latest">release page</a>.</p>
    <div class="install">
      <Terminal title="shell" text={`$ npm i -g agent-browser && agent-browser install\n$ ${install}\n$ qaspec init     # qaspec.toml and specs/home.qa.ts\n$ qaspec check    # specs, config and secrets, no browser\n$ qaspec run`} />
      <button class="btn" onclick={copy} aria-live="polite">
        {#if copied}Copied{:else}Copy install command{/if}
      </button>
    </div>
  </section>

  <section class="wrap" id="config">
    <h2>Projects, environments and credentials</h2>
    <p class="sub">Base URLs, users and secrets live in <code>qaspec.toml</code>. Specs only name them. The <code>examples/shop</code> project is a store and its back office:</p>
    <div class="tabs" role="tablist" aria-label="Example files">
      {#each Object.entries(files) as [k, [label]]}
        <button role="tab" id="tab-{k}" aria-selected={tab === k} aria-controls="panel" tabindex={tab === k ? 0 : -1} onclick={() => (tab = k)}>{label}</button>
      {/each}
    </div>
    <div id="panel" role="tabpanel" aria-labelledby="tab-{tab}">
      {#key tab}<Code code={files[tab][1]} lang={files[tab][2]} />{/key}
    </div>
    <dl class="rows">
      <div><dt>Switch environment</dt><dd><code>--env staging</code> or <code>QASPEC_ENV</code></dd></div>
      <div><dt>Override a value</dt><dd><code>--set params.product=…</code>, <code>--set projects.store.base_url=…</code></dd></div>
      <div><dt>A project is down</dt><dd>Its <code>health</code> check fails and its suites, plus the projects that depend on it, are reported as blocked rather than failed.</dd></div>
      <div><dt>Journey across apps</dt><dd><code>step('…', {'{'} project: 'store' {'}'})</code> switches to that app's tab, which keeps its page and session.</dd></div>
      <div><dt>Pass data on</dt><dd><code>capture('orderNumber', …)</code>, then <code>{'${orderNumber}'}</code> in later steps and files.</dd></div>
      <div><dt>Two users in one app</dt><dd>Suites are grouped by user. Switching saves one sign-in and restores the other, in the same browser.</dd></div>
    </dl>
    <Figure src="app-admin" width="1920" height="1120" alt="Admin app showing the total number of todos" caption="The back-office app in the demo. A step with <code>project: 'admin'</code> opens it in its own tab and checks the todo count captured in the store." />
  </section>

  <section class="wrap" id="reference">
    <h2>Expectations</h2>
    <p class="sub">Exact checks run first and cost nothing. The model judges only if they pass.</p>
    <div class="ref">
      <table>
        <thead><tr><th scope="col">Call</th><th scope="col">Checks</th><th scope="col">Model</th></tr></thead>
        <tbody>
          <tr><td><code>expect('…')</code></td><td>The screen and this step's signals. The judge must quote evidence.</td><td>1 call</td></tr>
          <tr><td><code>expect.console.noErrors()</code></td><td>No <code>console.error</code> or uncaught error during the step</td><td>None</td></tr>
          <tr><td><code>expect.network('POST /api/**').status(201)</code></td><td>Requests made during the step</td><td>None</td></tr>
          <tr><td><code>expect.network.noFailures()</code></td><td>No response with status 400 or above</td><td>None</td></tr>
          <tr><td><code>expect.state('window.x').equals(1)</code></td><td>Any JavaScript expression in the page</td><td>None</td></tr>
          <tr><td><code>expect.url().toContain('/x')</code></td><td>The current URL</td><td>None</td></tr>
          <tr><td><code>expect.visible('Welcome')</code></td><td>Text on the page</td><td>None</td></tr>
          <tr><td><code>expect.storage.local('k').equals('v')</code></td><td><code>localStorage</code></td><td>None</td></tr>
        </tbody>
      </table>
    </div>
    <p class="note">The full reference is in the <a href="{repo}#spec-reference">README</a>. Exit codes: 0 passed, 1 failed, 2 blocked, 3 configuration error.</p>
  </section>
</main>

<footer>
  <div class="wrap foot">
    <span>qaspec {version}, MIT licensed</span>
    <span><a href={repo}>Source</a> · <a href="{repo}/releases">Releases</a> · <a href="https://github.com/vercel-labs/agent-browser">agent-browser</a></span>
  </div>
</footer>

<style>
  .skip { position: absolute; left: -9999px; top: 8px; z-index: 30; background: var(--text); color: var(--bg); padding: 8px 12px; border-radius: var(--r-sm); }
  .skip:focus { left: 12px; }

  .top { position: sticky; top: 0; z-index: 20; background: rgb(11 13 16 / 0.92); border-bottom: 1px solid var(--border); }
  @supports (backdrop-filter: blur(1px)) { .top { background: rgb(11 13 16 / 0.8); backdrop-filter: blur(10px); } }
  nav { display: flex; align-items: center; gap: 8px; height: 56px; }
  .logo { font: 500 18px var(--mono); color: var(--text); margin-right: auto; padding: 10px 0; min-height: 44px; display: inline-flex; align-items: center; }
  .logo span { color: var(--accent); }
  .logo:hover { text-decoration: none; }
  .links { display: flex; gap: 4px; }
  .links a, .gh { color: var(--text-2); font-size: 14px; padding: 10px 10px; min-height: 40px; display: inline-flex; align-items: center; border-radius: var(--r-sm); transition-property: color, background-color; transition-duration: 150ms; transition-timing-function: var(--ease); }
  .links a:hover, .gh:hover { color: var(--text); background: var(--overlay); text-decoration: none; }
  .gh { display: inline-flex; align-items: center; gap: 8px; color: var(--text); }
  .burger { display: none; }

  section { padding: 56px 0; border-top: 1px solid var(--border); }
  h2 { font-size: 26px; line-height: 1.25; font-weight: 600; letter-spacing: -0.01em; margin: 0 0 8px; }
  h3 { font-size: 16px; font-weight: 500; margin: 0; }
  .sub { color: var(--text-2); margin: 0 0 28px; max-width: 70ch; }
  .note { color: var(--text-3); font-size: 14px; margin: 18px 0 0; max-width: 80ch; }

  .hero { border-top: 0; padding-top: 64px; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.1fr); gap: 48px; align-items: center; }
  h1 { font-size: 44px; line-height: 1.1; font-weight: 600; letter-spacing: -0.02em; margin: 0 0 18px; }
  .lead { color: var(--text-2); font-size: 18px; margin: 0 0 28px; max-width: 52ch; }
  .cta { display: flex; flex-wrap: wrap; gap: 10px; }
  .facts { margin: 20px 0 0; color: var(--text-3); font-size: 14px; }

  .btn {
    display: inline-flex; align-items: center; justify-content: center; min-height: 44px; padding: 0 18px;
    border-radius: 8px; font: 500 15px var(--sans); color: var(--text); background: var(--raised);
    box-shadow: 0 0 0 1px var(--border-strong); border: 0; cursor: pointer;
    transition-property: background-color, scale; transition-duration: 150ms; transition-timing-function: var(--ease);
  }
  .btn:hover { background: var(--overlay); text-decoration: none; }
  .btn:active { scale: 0.96; }
  .btn.primary { background: var(--accent); color: var(--accent-ink); box-shadow: none; }
  .btn.primary:hover { background: #4ac85b; }

  .flow { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 32px; }
  .flow h3 { margin: 16px 0 6px; display: flex; align-items: baseline; gap: 10px; }
  .flow .n { color: var(--text-3); font: 500 14px var(--mono); }
  .flow p { margin: 0; color: var(--text-2); font-size: 15px; }

  .demo { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 20px; align-items: start; }

  .tbl-h { margin: 32px 0 12px; color: var(--text); }
  .versus { display: none; }

  .install { max-width: 760px; display: grid; gap: 14px; justify-items: start; }

  .tabs { display: flex; gap: 2px; overflow-x: auto; margin-bottom: 8px; scrollbar-width: none; }
  .tabs::-webkit-scrollbar { display: none; }
  .tabs button {
    flex: none; min-height: 40px; padding: 0 14px; border: 0; border-radius: var(--r-sm); cursor: pointer;
    font: 13px var(--mono); color: var(--text-2); background: transparent;
    transition-property: color, background-color; transition-duration: 150ms; transition-timing-function: var(--ease);
  }
  .tabs button:hover { color: var(--text); background: var(--overlay); }
  .tabs button[aria-selected='true'] { color: var(--text); background: var(--raised); box-shadow: inset 0 0 0 1px var(--border-strong); }

  .rows { margin: 32px 0; }
  .rows div { display: grid; grid-template-columns: 220px 1fr; gap: 16px; padding: 14px 0; border-top: 1px solid var(--border); }
  .rows div:last-child { border-bottom: 1px solid var(--border); }
  .rows dt { color: var(--text); font-weight: 500; }
  .rows dd { margin: 0; color: var(--text-2); }

  .ref { border-radius: var(--r-md); box-shadow: 0 0 0 1px var(--border); overflow-x: auto; background: var(--surface); }
  .ref table { width: 100%; border-collapse: collapse; font-size: 14px; min-width: 560px; }
  .ref th { text-align: left; font-weight: 500; color: var(--text); background: var(--raised); padding: 11px 14px; border-bottom: 1px solid var(--border); }
  .ref td { padding: 11px 14px; color: var(--text-2); vertical-align: top; }
  .ref tr + tr td { border-top: 1px solid var(--border); }
  .ref td:first-child code { white-space: nowrap; }

  footer { border-top: 1px solid var(--border); padding: 28px 0 40px; color: var(--text-3); font-size: 14px; }
  .foot { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px 24px; }
  footer a { color: var(--text-2); }

  @media (max-width: 960px) {
    .hero, .demo { grid-template-columns: minmax(0, 1fr); }
    .hero { gap: 32px; padding-top: 40px; }
    .flow { grid-template-columns: minmax(0, 1fr); gap: 36px; }
    .flow li { display: grid; grid-template-columns: minmax(0, 1fr); }
  }
  @media (max-width: 760px) {
    .links { display: none; position: absolute; top: 56px; left: 0; right: 0; flex-direction: column; gap: 0; padding: 8px 12px 12px; background: var(--surface); border-bottom: 1px solid var(--border); }
    .links.open { display: flex; }
    .links a { min-height: 44px; display: flex; align-items: center; font-size: 15px; }
    .gh span { display: none; }
    .gh { width: 44px; height: 44px; justify-content: center; padding: 0; }
    .burger { display: inline-flex; align-items: center; justify-content: center; width: 44px; height: 44px; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--text); cursor: pointer; }
    section { padding: 40px 0; }
    h1 { font-size: 32px; }
    h2 { font-size: 22px; }
    .lead { font-size: 16px; }
    .cta .btn { flex: 1 1 auto; }
    .rows div { grid-template-columns: minmax(0, 1fr); gap: 4px; }
    .versus {
      display: flex; align-items: center; gap: 4px; overflow-x: auto; scrollbar-width: none;
      position: sticky; top: 56px; z-index: 5; margin: 0 0 4px; padding: 8px 0; background: var(--bg); border-bottom: 1px solid var(--border);
    }
    .versus::-webkit-scrollbar { display: none; }
    .versus span { flex: none; color: var(--text-3); font-size: 13px; margin-right: 4px; }
    .versus button {
      flex: none; min-height: 40px; padding: 0 12px; border: 0; border-radius: var(--r-sm); cursor: pointer;
      font: 500 14px var(--sans); color: var(--text-2); background: transparent;
    }
    .versus button[aria-pressed='true'] { color: var(--text); background: var(--raised); box-shadow: inset 0 0 0 1px var(--border-strong); }
  }
  @media (prefers-reduced-motion: reduce) { .btn, .links a, .tabs button { transition: none; } .btn:active { scale: none; } }
</style>
