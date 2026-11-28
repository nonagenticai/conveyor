#!/usr/bin/env python3
"""Derive the tranche-review evidence from a snapshot data dir.

The review row (`tranches.review`) validates its evidence strictly
(`crates/domain/src/spine.rs::tranche_review_has_evidence` and
`crates/store/src/spine.rs::resolve_tranche_review_evidence_tx`):

  - `baselines`      non-empty array of UUID strings, each an approved
                     management|governance|boundary `programme_baseline` row of the programme;
  - `benefit_status` non-empty array of UUID strings, each a `benefit_profile` (or measurement)
                     of the programme;
  - `risk_register`  non-empty array of UUID strings, each a `risk` `project_register_item`
                     whose project belongs to the programme.

Anything else is descriptive context and rides under `detail` (the validator ignores it).
Exit 2 when a class is empty — a review with an empty class would be refused.
"""
import hashlib
import json
import sys
from pathlib import Path

snap = Path(sys.argv[1])
members = sys.argv[2].split(',') if len(sys.argv) > 2 else ['S04', 'T13', 'T3']

prog = json.loads((snap / 'programme.json').read_text())
msp = json.loads((snap / 'msp.json').read_text())
manifest_bytes = (snap / 'manifest.json').read_bytes()
manifest = json.loads(manifest_bytes)

row = prog['programme']
if isinstance(row, list):
    if not row:
        sys.exit('programme.json carries no programme row')
    row = row[0]
programme_id = row['id']
id2key = {p['id']: p['key'] for p in prog['projects']}

# ── the three validated classes ─────────────────────────────────────────────────────────
baseline_ids = [
    b['id'] for b in msp.get('programme_baselines', [])
    if b.get('programme_id') == programme_id
    and b.get('kind') in ('management', 'governance', 'boundary')
    and b.get('approved_by')
]
benefit_ids = [
    p['id'] for p in msp.get('benefit_profiles', [])
    if p.get('programme_id') == programme_id
]
member_ids = {p['id'] for p in prog['projects'] if p['key'] in members}
risk_ids = [
    r['id'] for r in msp.get('register_items', [])
    if r.get('kind') == 'risk' and r.get('project_id') in member_ids
]
for name, ids in (('baselines', baseline_ids), ('benefit_status', benefit_ids),
                  ('risk_register', risk_ids)):
    if not ids:
        sys.exit('evidence class %r is empty; the review would be refused' % name)

# ── descriptive context (not validated) ────────────────────────────────────────────────
project_baselines = {k: [] for k in members}
for b in msp.get('project_baselines', []):
    key = id2key.get(b.get('project_id'))
    if key in project_baselines:
        project_baselines[key].append({'id': b['id'], 'kind': b.get('kind'),
                                       'approved': bool(b.get('approved_by'))})

profiles = msp.get('benefit_profiles', [])
realisation = msp.get('benefit_realisation') or []
items = msp.get('register_items', [])
risks = [x for x in items if x.get('kind') == 'risk']
tranches = msp.get('tranches', [])
tranche_row = next((t for t in tranches if t.get('number') == 3), {})
tr4_row = next((t for t in tranches if t.get('number') == 4), {})
cadences = [c for c in msp.get('cadences', []) if c.get('period') == 'tranche-3']

print(json.dumps({
    'snapshot_sha': manifest.get('git_sha'),
    'snapshot_manifest_sha256': hashlib.sha256(manifest_bytes).hexdigest(),
    'snapshot_migration': manifest.get('migration'),
    'baselines': baseline_ids,
    'benefit_status': benefit_ids,
    'risk_register': risk_ids,
    'detail': {
        'project_baselines': project_baselines,
        'benefit': {
            'profiles': len(profiles),
            'active': [p.get('reference') for p in profiles if p.get('status') == 'active'],
            'draft': [p.get('reference') for p in profiles if p.get('status') == 'draft'],
            'measurements': sum(1 for r in realisation if r.get('latest_post')),
            'realisation_rows': len(realisation),
        },
        'risk': {
            'rows': len(items),
            'risks': len(risks),
            'open': sum(1 for x in items if x.get('status') == 'open'),
        },
        'programme_id': programme_id,
        'tranche_3': {'id': tranche_row.get('id'), 'state': tranche_row.get('state'),
                      'number': tranche_row.get('number')},
        'tranche_4': {'id': tr4_row.get('id'), 'state': tr4_row.get('state'),
                      'number': tr4_row.get('number')},
        'cadences': [{'id': c.get('id'), 'kind': c.get('kind'), 'state': c.get('state')}
                     for c in cadences],
    },
}, indent=1))
