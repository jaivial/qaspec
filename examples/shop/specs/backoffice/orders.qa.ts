import { suite, step, goal, expect } from 'qaspec';

// Runs after the store project (depends_on) in the same browser, in its own tab, and uses
// `${orderNumber}` captured by specs/store/checkout.qa.ts earlier in the run.
suite('admin sees the new order', { project: 'backoffice', as: 'admin', start: '/orders' }, () => {
  step('order is listed', () => {
    goal('find order ${orderNumber}');
    expect('order ${orderNumber} is listed with status Paid');
    expect.errors.none();
  });

  step('back to the store', { project: 'store', as: 'buyer', start: '/' }, () => {
    expect('the buyer is still signed in');
  });
});
