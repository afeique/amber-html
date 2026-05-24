// Smoke test for the AmberHTML Node binding (Plans.md 6.3, 10.2).
//
// Requires a locally-built addon (`amber.node` in the package dir). Uses an
// obviously-invalid URL so it returns instantly without any network/browser —
// it proves the addon loads, the widened surface is present, and the error
// contract surfaces as a JS exception.
'use strict';
const assert = require('node:assert');
const amber = require('..');

// The full surface is exported (Plans.md 10.2 — parity with UniFFI/C ABI).
for (const name of [
  'capture',
  'captureText',
  'save',
  'captureMarkdown',
  'captureReadable',
  'snapshot',
]) {
  assert.strictEqual(typeof amber[name], 'function', `${name} exported`);
}
assert.strictEqual(typeof amber.Snapshot, 'function', 'Snapshot class exported');
assert.strictEqual(typeof amber.Format, 'object', 'Format enum exported');
assert.strictEqual(amber.Format.Markdown, 2, 'Format.Markdown maps to its selector');
assert.strictEqual(amber.Format.Pdf, 7, 'Format.Pdf maps to its selector');

// Every entry point surfaces a bad URL as a clear JS exception (no panic).
assert.throws(
  () => amber.captureMarkdown('not a url'),
  /invalid URL/i,
  'captureMarkdown: a bad URL should throw',
);
assert.throws(
  () => amber.capture('not a url', amber.Format.Pdf),
  /invalid URL/i,
  'capture: a bad URL should throw',
);
assert.throws(
  () => amber.captureText('not a url', amber.Format.Markdown),
  /invalid URL/i,
  'captureText: a bad URL should throw',
);
assert.throws(
  () => amber.save('not a url', amber.Format.Html, '/tmp', 'amber_node_smoke'),
  /invalid URL/i,
  'save: a bad URL should throw',
);
assert.throws(
  () => amber.snapshot('not a url', [amber.Format.Markdown]),
  /invalid URL/i,
  'snapshot: a bad URL should throw',
);

console.log('amber-node smoke test passed');
