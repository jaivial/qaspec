import { suite, step, goal, expect, capture } from 'qaspec';

// One journey, several checks in order, one browser, signed in once.
suite('buyer checks out a product', { project: 'store', as: 'buyer', start: '/' }, () => {

  step('find the product', () => {
    goal('search for "${params.product}" and open its product page');
    expect('the product page of "${params.product}" shows a price and an Add to cart button');
    expect.network('GET /api/products/**').status(200);
    expect.console.noErrors();
  });

  step('add it to the cart', () => {
    goal('add one unit to the cart');
    expect('the cart badge shows 1 item');
    expect.network('POST /api/cart/**').status(201);
    expect.storage.local('cartId').exists();
  });

  step('pay', () => {
    goal('go to checkout and pay with the saved card');
    expect('an order confirmation with an order number is shown');
    expect.network.noFailures();
    capture.url('orderUrl');
    capture('orderNumber', 'the order number on the confirmation page');
  });

  // Independent of the previous state thanks to `start`; runs even if checkout failed.
  step('order history lists it', { start: '/account/orders', needs: ['pay'] }, () => {
    expect.visible('${orderNumber}');
  });
});
