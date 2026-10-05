import { suite, step, expect } from 'qaspec';

suite('admin overview', { start: '/' }, () => {
  step('shows the todo count captured in the app project', () => {
    expect.visible('Todos in the system: ${count}');
    expect.errors.none();
  });
});
