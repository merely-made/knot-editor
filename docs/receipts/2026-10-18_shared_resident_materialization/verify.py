#!/usr/bin/env python3
"""Read-only verification of retained Knot shared-reader qualification."""
from pathlib import Path
import hashlib
import json
import subprocess

packet = Path(__file__).resolve().parent
repo = packet.parents[2]
read = lambda p: json.loads((packet / p).read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
for ledger in ('artifact-sha256.json', 'final-artifact-sha256.json'):
    for path, expected in read(ledger)['files'].items():
        assert sha(packet / path) == expected, path
prior = read('final/prior-receipt-preservation.json')
for path, expected in prior['files'].items():
    assert sha(repo / path) == expected, path
    assert (repo / path).read_bytes() == subprocess.check_output(['git', 'show', prior['baseline'] + ':' + path], cwd=repo)
inputs = read('final/candidate-inputs.json')
for path, expected in inputs['files'].items():
    assert sha(repo / path) == expected, path
    assert (repo / path).read_bytes() == subprocess.check_output(['git', 'show', inputs['source'] + ':' + path], cwd=repo)
q = read('qualification.json')
assert q['source'] == inputs['source']
assert all(g['exit'] == 0 for g in q['gates'])
subprocess.run(['git', 'diff', '--exit-code', q['gates'][1]['source'], q['source'], '--', 'crates/knot-editor', 'Cargo.toml', 'Cargo.lock'], cwd=repo, check=True, stdout=subprocess.PIPE)
for phase, count in [('live-native', 3), ('live-native-reopen', 3), ('standalone', 1)]:
    root = 'native/knot-native-qualified-08/' + phase
    p = read(root + '/process.json')
    assert p['success'] and p['launcher_exit'] == 0 and not p['timeout']
    assert p['captures'] == count and p['binary_sha256'] == q['native_binary_sha256']
    assert (packet / root / 'scenario.done').read_text().startswith('RESULT ok\n')
reviews = read('final/capture-reviews.json')
assert len(reviews) == 7
for review in reviews:
    assert review['review'] and sha(packet / review['file']) == review['sha256']
fork = read('native/knot-native-qualified-08/live-native-reopen/resident-fork.json')
assert len(fork['sessions']) == 5
assert sum(s['name'].startswith('Session (fork of ') for s in fork['sessions']) == 1
assert any(s['id'] == fork['fork'] and s['name'] == fork['name'] for s in fork['sessions'])
assert 'host exit=0' in (packet / 'native/knot-native-qualified-08/logs/fixture-host.log').read_text()
print(f'Knot shared reader verified: {len(prior["files"])} prior files, {len(inputs["files"])} candidate inputs, seven reviewed captures, exact fork identity, fresh client reopening')
