import { suite, step, expect } from 'qaspec';

// Regression: a suite WITHOUT a device must run as a desktop browser, still signed in, even
// when a mobile suite is in the same run.
//
// agent-browser 0.27 cannot undo `set device`: the mobile user agent survives `set viewport`,
// and the only real undo (`--user-agent ""`) relaunches the browser, closing every tab and
// dropping the cookies. qaspec therefore plans device suites last, so this suite runs first
// and the checks below are about the session, not only the size.
suite('desktop after mobile', { project: 'app', as: 'qa', start: '/' }, () => {
  step('desktop size and user agent', () => {
    expect.state('window.innerWidth > 1000').toBeTruthy();
    expect.state('navigator.userAgent.includes("iPhone")').toBeFalsy();
    expect.state('navigator.userAgent.includes("Android")').toBeFalsy();
  });

  step('the session survived the mobile suite', () => {
    expect.url().not.toContain('/login');
    expect.visible('Welcome, qa@example.com');
  });
});
