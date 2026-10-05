import { suite, step, expect, capture } from 'qaspec';

// One journey across two projects: each project keeps its own tab in the same browser.
suite('cross-project journey', { as: 'qa', start: '/todos' }, () => {
  step('count todos in the app', () => {
    expect.network('GET /api/todos').status(200);
    capture.state('n', 'window.appState.count');
  });
  step('admin shows the same count', { project: 'admin' }, () => {
    expect.visible('Todos in the system: ${n}');
  });
  step('app tab kept its page and session', { project: 'app', as: 'qa' }, () => {
    expect.url().toContain('/todos');
    expect.visible('Buy milk');
  });
});
