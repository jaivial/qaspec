import { suite, step, expect } from 'qaspec';

// Regression: a suite WITHOUT a device must run as a desktop browser even if a mobile suite
// ran before it in the same agent-browser session. `set device` overrides the user agent and
// agent-browser cannot undo it with `set viewport`, so qaspec restores it explicitly.
suite('desktop after mobile', { project: 'app', as: 'qa', start: '/' }, () => {
  step('desktop size and user agent', () => {
    expect.state('window.innerWidth > 1000').toBeTruthy();
    expect.state('navigator.userAgent.includes("iPhone")').toBeFalsy();
    expect.state('navigator.userAgent.includes("Android")').toBeFalsy();
  });
});
