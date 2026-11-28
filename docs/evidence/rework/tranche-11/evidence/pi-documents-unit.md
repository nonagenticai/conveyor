# PI documents carry their unit — tranche 11

The trap: `verify-project.py::_published_for_period` matches the literal `tranche-11` in a PI
output's title and unit. A composed PI document without `unit` is invisible to the check, so
`pi_plan` and `pi_objectives` are red for every member even though the documents exist.

Both inputs carry `"unit": "tranche-11"`, and the read-back proves it in the platform outputs index:

```
python3 -c 'import json,re; d=json.loads(re.search(r"=\s*(\{.*\})\s*;?\s*$", open("/tmp/snap-11/docs/outputs.js").read(), re.S).group(1)); rows=[x for x in d["outputs"] if x.get("unit")=="tranche-11"]; print([(x["category"], x["project_key"], x["title"]) for x in rows])'
```

Both rows are present: `pi-plan` and `pi-objectives`, project key `P0`, title naming tranche 11.
