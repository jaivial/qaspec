import { suite, step, expect, capture } from 'qaspec';

// Mobile web: the suite runs on an emulated phone (`device` below) and every check is
// deterministic, so no model call is needed. They prove the emulation really reaches the page
// the agent drives: CSS size, pixel ratio and the mobile user agent.
suite('mobile', { as: 'qa', start: '/', device: 'iPhone 14' }, () => {
  step('the page runs at the size of the emulated phone', () => {
    expect.state('window.innerWidth').equals(390);
    expect.state('window.innerHeight').equals(844);
    expect.state('window.devicePixelRatio').equals(3);
    expect.state('document.documentElement.clientWidth <= window.innerWidth').toBeTruthy();
    expect.console.noErrors();
  });

  step('the app is served the mobile user agent', () => {
    expect.state('navigator.userAgent.includes("iPhone")').toBeTruthy();
    expect.state('navigator.userAgent.includes("Mobile")').toBeTruthy();
    expect.state('navigator.maxTouchPoints').toBeFalsy();
  });

  step('the navigation still works on a small screen', { start: '/todos' }, () => {
    expect.url().not.toContain('/login');
    expect.state('window.innerWidth').equals(390);
    expect.visible('Todos');
    expect.state('window.appState.count').toBeTruthy();
    capture.state('width', 'window.innerWidth');
  });
});
