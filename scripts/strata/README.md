# The local `strata` profile

`docker compose --profile strata up -d --wait strata-db strata-rag` starts a real `strata_rag`
(the sibling checkout at `../strata_rag`, built from its own Dockerfile) on **:8092** with its
Postgres on **:5440**. It is the harness behind S15's knowledge/memory proofs — no stub stands in
for strata itself.

The API binds `0.0.0.0:8092` on purpose: builds and tests run on **m1**, which reaches mini3 over
the LAN. It is token-protected; the dev token is `conveyor-strata-dev` and the tenant is named per
request in `X-Tenant-Id`.

`MLX_API_KEY` is read from the estate's SOPS secret, never committed:

```sh
export MLX_API_KEY=$(cd ~/Movies/gitops-production/clusters/strato/services/strata-rag \
  && sops -d --extract '["stringData"]["MLX_API_KEY"]' secrets.sops.yaml)
docker compose --profile strata up -d --wait strata-db strata-rag
curl -s http://127.0.0.1:8092/health
# {"db":"ok","embed":"ok","rerank":"ok","chat":"unconfigured","laya":"unconfigured"}
```

`chat` is deliberately unconfigured: the conveyor's `knowledge.query` never calls `/api/ask`.

## Proving it end to end (measured 2026-11-09)

```sh
T=0197a0c0-0000-7000-8000-0000000000a1
H=(-H "Authorization: Bearer conveyor-strata-dev" -H "X-Tenant-Id: $T" \
   -H 'content-type: application/json')

# a parentless LEAF is the tenant's single store; it holds documents directly
S=$(curl -s -X POST http://127.0.0.1:8092/api/stores "${H[@]}" \
      -d '{"kind":"leaf","name":"tenant-single","description":""}' | jq -r .id)

curl -s -X POST "http://127.0.0.1:8092/api/stores/$S/documents" "${H[@]}" \
  -d '{"text":"a work package is a card","source":"docs","source_id":"conveyor:work:x",
       "source_run":"r1","metadata":{"capability":"design"}}'

curl -s -X POST http://127.0.0.1:8092/api/query "${H[@]}" \
  -d "{\"query\":\"what is a work package\",\"top_k\":3,\"leaf_ids\":[\"$S\"],
       \"metadata\":{\"capability\":\"design\"}}"
```

The last call returns one `tenant` hit. A parentless `router` store **cannot** be ingested into —
that is why the memory store is a leaf.

## What the tests use

- Transport/unit tests: a stub server, under `rbuild` — they prove the client's error mapping, not
  strata's behaviour.
- Live proofs (`#[ignore]`): `STRATA_LIVE_URL` defaults to `http://mini3.local:8092` with the dev
  token above; `rbuild` does not forward arbitrary environment variables, which is why the default
  is in the test.
