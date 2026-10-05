import { suite, step, goal, expect } from 'qaspec';

// Runs only when the saved session for store/buyer is missing or expired.
suite('store: sign in buyer', { start: '/login' }, () => {
  step('sign in', () => {
    goal('sign in with ${identity.username} and the password secret');
    expect.url().not.toContain('/login');
    expect.errors.none();
  });
});
