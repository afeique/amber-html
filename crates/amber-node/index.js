// Loads the AmberHTML native addon (Plans.md 6.3).
//
// Built via `npm run build` (@napi-rs/cli), or for local dev:
//   cargo build -p amber-node && cp ../../target/debug/libamber_node.dylib amber.node
// Exposes captureMarkdown(url) and captureReadable(url).
'use strict';
const { existsSync } = require('node:fs');
const { join } = require('node:path');

// A local dev build (`amber.node`) first, then platform-named prebuilds.
const candidates = [
  'amber.node',
  'amber.darwin-arm64.node',
  'amber.darwin-x64.node',
  'amber.linux-x64-gnu.node',
  'amber.linux-arm64-gnu.node',
];

let addon;
for (const name of candidates) {
  const path = join(__dirname, name);
  if (existsSync(path)) {
    addon = require(path);
    break;
  }
}
if (!addon) {
  throw new Error(
    'amber native addon not built — run `npm run build` (napi) or ' +
      '`cargo build -p amber-node` and copy the cdylib to amber.node',
  );
}

module.exports = addon;
