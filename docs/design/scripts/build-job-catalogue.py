"""Build a display snapshot of historical jobs, preserving their source contracts."""
import hashlib
import json
import re
from pathlib import Path

root = Path(__file__).resolve().parents[3]
source = root.parent / 'requirements_management/src/data/pm_catalogue_defaults.json'
design = root.parent / 'n8n-designs/design/choices/v4/capabilities'
raw = source.read_bytes()
data = json.loads(raw)
jobs = {x['name']: dict(x, origin='RM / n8n v4 seed') for x in data['card_types']}
for path in sorted(design.glob('*.yaml')):
    content = path.read_text()
    capability = re.search(r'^capability: (\S+)', content, re.M)
    for code in re.findall(r'^  ([a-z][a-z0-9_]+):', content.split('\ninputs:')[0].split('card_types:', 1)[-1], re.M):
        if code not in jobs:
            jobs[code] = dict(name=code, title=code.replace('_', ' ').capitalize(), capability=capability[1] if capability else path.stem, description='Historical design definition; requires tenant review before use.', inputs=[], outputs=[], acceptance=[], origin='n8n v4 design / ' + path.name)
registry = root.parent / 'n8n-designs/design/choices/v3/vendor/registry_card_types.txt'
for code in registry.read_text().splitlines():
    if code and code not in jobs:
        jobs[code] = dict(name=code, title=code.replace('_', ' ').capitalize(), capability='unmapped', description='Historical registry seed; capability and process binding need tenant review.', inputs=[], outputs=[], acceptance=[], origin='n8n v3 historical registry')
out = dict(source='../requirements_management/src/data/pm_catalogue_defaults.json', sha256=hashlib.sha256(raw).hexdigest(), design='../n8n-designs/design/choices/v4/capabilities/*.yaml', capabilities=data['capabilities'], jobs=list(jobs.values()))
(root / 'docs/design/job-catalogue.js').write_text('// Local display snapshot. Regenerate with scripts/build-job-catalogue.py.\nwindow.JOB_CATALOGUE = ' + json.dumps(out, ensure_ascii=False, indent=2) + ';\n')
print(f"Extracted {len(jobs)} historical job definitions across {len(data['capabilities'])} capability seeds.")
