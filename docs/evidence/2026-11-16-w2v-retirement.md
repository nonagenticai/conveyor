# The dead w2v estate retired — 2026-11-17

Part of #213. Its single `blocked-by` (#205, the research cutover) closed on 2026-11-17 at
14:30Z; the deployment half merged the same evening as
[gitops-production#5323](https://github.com/nonagenticai/gitops-production/pull/5323)
(`22d4b10`), Flux reconciled, and this document records what changed.

**What was wrong.** The 17 Deployments in namespace `w2v` had reported `0/0` with no pod for
weeks, and the 16 Services answered only those dead backends. `studio.nonagentic.ai` therefore
returned **503** — envoy's canned `upstream connect error` — instead of a deliberate status. A
host that answers 503 into nothing looks like an outage that nobody is watching; a 404 is a
statement.

## What was removed, and what deliberately was not

| Removed | Count | Why |
|---|---|---|
| `services/w2v/deployments.yaml` | 17 Deployments | `0/0`, no pod, 137–192 days old |
| `services/w2v/services.yaml` | 16 Services | every one answered only the above |
| `services/w2v/poddisruptionbudget.yaml` | 1 PDB set | over Deployments that no longer exist |
| the `replicas:` transformer + ~800-line `patches:` block | — | every patch targeted a Deployment removed here |
| `services-istio/w2v/virtualservice.yaml` → VS `w2v` | 1 VS | it routed `studio.nonagentic.ai` and `w2v.internal` at the dead backends |
| `services-istio/w2v/destinationrule.yaml` | 1 DR | its only subject was `w2v-frontend` |

**Kept, and stated in the kustomization:** the namespace, `pv-worker3.yaml`, `storage.yaml`
(**the data**), the default-deny `networkpolicy.yaml`, the ConfigMaps and the Secrets, and the
`w2v-legacy-redirect` VirtualService (which carries no backend — it is why `w2v.nonagentic.ai`
still answers at all). This unit deletes no data; every manifest is one revert away.

## Evidence at completion

### P1.1 — every Deployment on the list is `0/0` with no pod

```
$ kubectl -n w2v get deploy,pods
NAME                                            READY   UP-TO-DATE   AVAILABLE   AGE
deployment.apps/ai-content-generation-service   0/0     0            0           187d
deployment.apps/analysis-service                0/0     0            0           192d
deployment.apps/app-inspector-service           0/0     0            0           192d
deployment.apps/bdd-playwright-service          0/0     0            0           192d
deployment.apps/content-service                 0/0     0            0           192d
deployment.apps/frontend-test-runner-service    0/0     0            0           192d
deployment.apps/html-render-service             0/0     0            0           167d
deployment.apps/redis                           0/0     0            0           192d
deployment.apps/slide-service                   0/0     0            0           192d
deployment.apps/tts-gateway                     0/0     0            0           138d
deployment.apps/tts-gateway-worker              0/0     0            0           137d
deployment.apps/tts-gateway-worker-mini1        0/0     0            0           137d
deployment.apps/video-analyzer-service          0/0     0            0           192d
deployment.apps/video-production-service        0/0     0            0           192d
deployment.apps/w2v-frontend                    0/0     0            0           192d
deployment.apps/w2v-orchestrator                0/0     0            0           192d
deployment.apps/youtube-upload-service          0/0     0            0           192d
```

No pod lines in that output: there were none. 16 Services were live (`kubectl -n w2v get svc`
→ 16) and 2 VirtualServices carried the hostnames above.

### P1.2 — the survivors, each named with its reason

| Survivor | Why it stays |
|---|---|
| namespace `w2v` | still holds the PVs, the secrets and the default-deny policy |
| `pv-worker3.yaml`, `storage.yaml` | **the data** — this unit deletes none |
| `networkpolicy.yaml` | the namespace keeps its default-deny whether or not a pod exists |
| `configmaps.yaml`, the 31 Secrets | the retired fleet's configuration and credentials |
| VS `w2v-legacy-redirect` | carries no backend; the only reason `w2v.nonagentic.ai` answers |
| `services-istio/w2v/peerauth.yaml` | namespace- and workload-scoped mTLS policy; inert over no workloads, and removing it is a decision about the namespace, not about this estate |

### P2.1 — after merge and reconcile

```
$ kubectl -n w2v get deploy,pods,svc
No resources found in w2v namespace.
$ kubectl -n w2v get vs
NAME                  GATEWAYS                                      HOSTS                 AGE
w2v-legacy-redirect   ["istio-system/api-external-gateway",…]       ["w2v.nonagentic.ai"] 27d
```

Flux: `kustomization services` → `main@sha1:76ba6478c7c2c5c05ead2b818406569e82acb05f`, then
`kustomization services-istio` pruned the VS.

### P2.2 — the hostnames answer deliberately

```
$ curl -sS -o /dev/null -w '%{http_code}' https://studio.nonagentic.ai
404          # was 503 before the change
$ curl -sS -o /dev/null -w '%{http_code}' https://w2v.nonagentic.ai
308          # unchanged: the legacy redirect carries no backend
$ getent hosts w2v.internal          # in-cluster, from a conveyor pod
100.81.177.99   w2v.internal        # the ingress address, unchanged
```

### P2.3 — the namespace's Redis is named by nobody else

```
$ kubectl get deploy -A -o json | (scan every spec for redis.w2v / w2v-redis / redis.w2v.svc)
namespaces scanned: 71
deployments scanned: 157
references to the w2v redis from other namespaces: none
```

### P3.1 — the docs gate

```
$ bash scripts/ci/no-private-refs.sh
OK: no private infrastructure references in 2912 tracked files (9 allowlisted).
```

## The gate consequences this unit produced

Removing 34 objects moved the deployment repository's non-vacuity floor: `build-all.sh` measured
**1925 → 1891** objects, below its 1900 floor. The floor moved to 1880 with its reason recorded
beside it, and the alert-coverage baseline — a ratchet, not a permanent exemption — lost its 17
now-orphaned `w2v/*` entries rather than being silenced. `build-all.sh` at the merged head:
*13 roots, 1891 objects, 0 violations.*

A `clusters/**` merge fires no workflow by design (#3930); the PR's `validate` job ran on the pull
request and passed, which is the run that matters.
