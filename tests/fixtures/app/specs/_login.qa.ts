import { suite, step, goal, expect } from 'qaspec';

suite('sign in', { start: '/login' }, () => {
  step('sign in', () => {
    goal('sign in with ${identity.username} and the password secret');
    expect.url().not.toContain('/login');
  });
});
