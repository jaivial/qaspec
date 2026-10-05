import { suite, step, expect, capture } from 'qaspec';

// Deterministic checks only (no LLM needed).
suite('smoke', { as: 'qa', start: '/' }, () => {
  step('home loads signed in', () => {
    expect.url().not.toContain('/login');
    expect.visible('Welcome, ${identity.username}');
    expect.console.noErrors();
  });
  step('todos page calls the API', { start: '/todos' }, () => {
    expect.network('GET /api/todos').status(200);
    expect.state('window.appState.count').toBeTruthy();
    capture.state('count', 'window.appState.count');
  });
  step('broken page is detected', { start: '/broken', onFail: 'continue' }, () => {
    expect.console.noErrors();
    expect.network('/api/report').ok();
  });
  step('still on the todos state', { needs: ['todos page calls the API'], start: '/todos' }, () => {
    expect.visible('Buy milk');
  });
});
