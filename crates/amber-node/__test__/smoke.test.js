// Smoke test for the AmberHTML Node binding (Plans.md 6.3).
//
// Requires a locally-built addon (`amber.node` in the package dir). Uses an
// obviously-invalid URL so it returns instantly without any network/browser —
// it proves the addon loads and the error contract surfaces as a JS exception.
'use strict';
const assert = require('node:assert');
const amber = require('..');

assert.strictEqual(typeof amber.captureMarkdown, 'function', 'captureMarkdown exported');
assert.strictEqual(typeof amber.captureReadable, 'function', 'captureReadable exported');

assert.throws(
  () => amber.captureMarkdown('not a url'),
  /invalid URL/,
  'a bad URL should throw a clear error',
);

console.log('amber-node smoke test passed');
