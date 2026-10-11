// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
// Read-only verification of the marked native acceptance fixture and receipts.
// Usage: node verify-native.cjs /absolute/path/to/live-dynamics-evidence
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const root = process.argv[2];
assert(root && path.isAbsolute(root), 'Supply the absolute native evidence root');
const binary = '50723e55be65ad89fcbe8f1b3bbea6b8dccd552e0a0c57597f924dc1cb0ec35d';
const authored = '48dea041f148dba84eeaafabccf2151101cfa34a258c0573198939f34832fd00';
const source = fs.readFileSync(path.join(root, 'qualified-fixture', 'essay.djot'));
assert.equal(source.toString('utf8'), '# Night light\n\nNight light by the river.\n');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const names = ['qualified-wide-seed', 'qualified-wide-reopen', 'qualified-compact-reopen'];
const runs = names.map((name, index) => {
  const directory = path.join(root, name);
  const processResult = JSON.parse(fs.readFileSync(path.join(directory, 'process.json'), 'utf8'));
  assert.equal(processResult.success, true, `${name} must pass`);
  assert.equal(processResult.binary_sha256, binary);
  const receipt = fs.readFileSync(path.join(directory, 'scenario.done'), 'utf8');
  assert(receipt.startsWith('RESULT ok\n'));
  assert(!receipt.includes('FAIL:'));
  const captures = Object.fromEntries(receipt.split('\n')
    .filter(line => line.startsWith('capture-pairing '))
    .map(line => {
      const match = line.match(/^capture-pairing (\S+) (\{.*\})$/);
      assert(match, 'Capture pairing must have a name and JSON');
      const item = JSON.parse(match[2]);
      assert.equal(item.width, index === 2 ? 840 : 2200);
      assert.equal(item.height, 1800);
      assert.deepEqual(item.fields, item.pairing.product_projection.fields);
      assert.equal(item.fields.source_bytes, '41');
      assert.equal(item.fields.scene_positions_blake3, authored);
      assert.equal(item.fields.scene_authored_positions, '1');
      assert.equal(item.fields.scene_placement_steps, '2');
      assert.equal(item.fields.scene_motion_roles, '1');
      assert.equal(item.fields.scene_motion_slot, 'true');
      return [match[1], {
        width: item.width,
        height: item.height,
        png_sha256: sha(fs.readFileSync(path.join(directory, `${match[1]}.png`))),
        presentation_sequence: item.pairing.presentation.sequence,
        fields: item.fields,
      }];
    }));
  assert.equal(Object.keys(captures).length, processResult.captures);
  assert.equal(processResult.captures, [7, 2, 6][index]);
  const suffix = index === 2 ? '-compact' : '';
  if (index !== 1) {
    const started = captures[`scene-motion-started${suffix}`].fields;
    const moving = captures[`scene-motion-running${suffix}`].fields;
    const paused = captures[`scene-motion-paused${suffix}`].fields;
    const stable = captures[`scene-motion-paused-stable${suffix}`].fields;
    assert.equal(moving.scene_motion_running, 'true');
    assert(+moving.scene_motion_ticks > +started.scene_motion_ticks);
    assert.notEqual(moving.scene_motion_digest, started.scene_motion_digest);
    assert.equal(paused.scene_motion_running, 'false');
    assert.equal(stable.scene_motion_running, 'false');
    assert.equal(paused.scene_motion_ticks, stable.scene_motion_ticks);
    assert.equal(paused.scene_motion_digest, stable.scene_motion_digest);
  }
  if (index !== 0) {
    const reopened = captures[index === 1
      ? 'relationship-live-dynamics-reopened'
      : 'relationship-live-dynamics-compact-reopened'].fields;
    assert.equal(reopened.scene_motion_running, 'false');
    assert.equal(reopened.scene_motion_ticks, '0');
    assert.equal(reopened.scene_motion_positions, '0');
  } else {
    const revealed = captures['scene-motion-revealed-paused'].fields;
    const retained = captures['relationship-live-dynamics-retained'].fields;
    assert.equal(revealed.scene_motion_running, 'false');
    assert.equal(retained.scene_motion_running, 'false');
    assert.equal(revealed.scene_motion_ticks, retained.scene_motion_ticks);
    assert.equal(revealed.scene_motion_digest, retained.scene_motion_digest);
  }
  const presentation = receipt.match(/^presentation: (\d+) frames, (\d+) redraws, (\d+) unpresented redraws$/m);
  assert(presentation);
  return { name, process: processResult,
    presented_frames: +presentation[1], redraws: +presentation[2],
    unpresented_redraws: +presentation[3], captures };
});
console.log(JSON.stringify({
  schema: 'knot.live-dynamics-native-qualification/v1',
  runtime_source: '552640f3a9afd6774705cc30762098bd67eef5a0',
  evidence_root: root, binary_sha256: binary,
  source_bytes: source.length, source_sha256: sha(source),
  authored_positions_blake3: authored,
  captures: runs.reduce((count, run) => count + run.process.captures, 0),
  tests: ['motion advances', 'paused pose is stable', 'hidden scene does not auto-resume',
    'fresh process has no runtime', 'authored state survives', 'source bytes unchanged'],
  runs,
}, null, 2));
