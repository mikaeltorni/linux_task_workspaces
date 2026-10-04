// Run: node --test tests/*.test.mjs
// Coverage: saved top-monitor placement, changed resolution, unplugged monitor
// fallback and bounds -> these public placement assertions.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {placement} from '../extension/geometry.js';
const old = [{index: 0, connector: 'DP-1', x: 0, y: -1080, width: 1920, height: 1080}];
const saved = {monitor: 'DP-1', rect: [960, -1080, 960, 1080]};
test('preserves top monitor and window halves', () => {
    assert.deepEqual(placement(saved, old, old).rect, saved.rect);
});
test('scales to changed resolution and falls back visibly', () => {
    const current = [{index: 0, connector: 'HDMI-1', x: 0, y: 0, width: 1280, height: 720}];
    assert.deepEqual(placement(saved, old, current).rect, [640, 0, 640, 720]);
});
