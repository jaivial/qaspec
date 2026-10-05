import { suite, step, expect } from 'qaspec';

// Two identities of the same project in one run: the runner saves qa's state, clears cookies,
// restores the other projects' identities and loads viewer's state (or signs viewer in).
suite('qa sees their name', { as: 'qa', start: '/' }, () => {
  expect.visible('Welcome, qa@example.com');
});

suite('viewer sees their name', { as: 'viewer', start: '/' }, () => {
  expect.visible('Welcome, viewer@example.com');
  expect.not.visible('qa@example.com');
});

suite('back to qa without signing in again', { as: 'qa', start: '/' }, () => {
  expect.visible('Welcome, qa@example.com');
});
