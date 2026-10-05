import { suite, step, goal, expect, capture } from 'qaspec';

// Agent-driven journey: several checks in order, sharing the browser state.
suite('todos journey', { as: 'qa', start: '/' }, () => {
  step('go to the todo list', () => {
    goal('open the Todos page from the navigation');
    expect('the Todos page lists at least one todo');
    expect.url().toContain('/todos');
  });

  step('add a todo', () => {
    goal('add a new todo with the text "${params.todo} ${run.id}"');
    expect('the list now contains "${params.todo} ${run.id}"');
    expect.network('POST /api/todos').status(201);
    expect.storage.local('lastTodo').equals('${params.todo} ${run.id}');
    expect.console.noErrors();
    capture('first', 'the text of the first todo in the list');
  });

  step('home reflects the new count', { start: '/' }, () => {
    expect('the welcome page says how many todos the user has, and it is more than 1');
    expect.visible('Welcome');
  });
});
