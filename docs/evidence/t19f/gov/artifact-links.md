# T19F artefact link — driver_goal_objective derivation requirements_catalog

Action `architecture.artifacts_link` from 01a210bb-f6ea-7d99-b23f-4dc3dace459c to 01a210bc-1f7b-7703-bb2d-a0d8695d12bd, kind `derivation` (docs/plans/T19F/architecture-design.md §Links).


```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a210bb-f6ea-7d99-b23f-4dc3dace459c",
  "id": "01a210bd-2f20-74b2-9362-c2c52f9387e5",
  "kind": "derivation",
  "to_artifact_id": "01a210bc-1f7b-7703-bb2d-a0d8695d12bd",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a210bb-f6ea-7d99-b23f-4dc3dace459c",
      "id": "01a210bd-2f20-74b2-9362-c2c52f9387e5",
      "kind": "derivation",
      "to_artifact_id": "01a210bc-1f7b-7703-bb2d-a0d8695d12bd",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a210bc-1f7b-7703-bb2d-a0d8695d12bd",
  "id": "01a210bd-427d-70d6-8990-fcd27c8d269a",
  "kind": "derivation",
  "to_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a210bb-f6ea-7d99-b23f-4dc3dace459c",
      "id": "01a210bd-2f20-74b2-9362-c2c52f9387e5",
      "kind": "derivation",
      "to_artifact_id": "01a210bc-1f7b-7703-bb2d-a0d8695d12bd",
      "version": 1
    },
    {
      "from_artifact_id": "01a210bc-1f7b-7703-bb2d-a0d8695d12bd",
      "id": "01a210bd-427d-70d6-8990-fcd27c8d269a",
      "kind": "derivation",
      "to_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
  "id": "01a210bd-56bf-7cac-b247-ca534b179239",
  "kind": "realization",
  "to_artifact_id": "01a210bc-92bd-71d7-964f-8d9761e78baa",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a210bc-1f7b-7703-bb2d-a0d8695d12bd",
      "id": "01a210bd-427d-70d6-8990-fcd27c8d269a",
      "kind": "derivation",
      "to_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
      "version": 1
    },
    {
      "from_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
      "id": "01a210bd-56bf-7cac-b247-ca534b179239",
      "kind": "realization",
      "to_artifact_id": "01a210bc-92bd-71d7-964f-8d9761e78baa",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
  "id": "01a210bd-6a41-7642-a2b5-fa70f7cf444c",
  "kind": "realization",
  "to_artifact_id": "01a210bc-6b09-7785-ab85-5e72fc64997a",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a210bc-1f7b-7703-bb2d-a0d8695d12bd",
      "id": "01a210bd-427d-70d6-8990-fcd27c8d269a",
      "kind": "derivation",
      "to_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
      "version": 1
    },
    {
      "from_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
      "id": "01a210bd-56bf-7cac-b247-ca534b179239",
      "kind": "realization",
      "to_artifact_id": "01a210bc-92bd-71d7-964f-8d9761e78baa",
      "version": 1
    },
    {
      "from_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
      "id": "01a210bd-6a41-7642-a2b5-fa70f7cf444c",
      "kind": "realization",
      "to_artifact_id": "01a210bc-6b09-7785-ab85-5e72fc64997a",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a210bb-cfc9-72ec-85f5-619bf730fff9",
  "id": "01a210bd-7d79-74f8-b53b-e9e5a1b8bef6",
  "kind": "dependency",
  "to_artifact_id": "01a210bc-6b09-7785-ab85-5e72fc64997a",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a210bb-cfc9-72ec-85f5-619bf730fff9",
      "id": "01a210bd-7d79-74f8-b53b-e9e5a1b8bef6",
      "kind": "dependency",
      "to_artifact_id": "01a210bc-6b09-7785-ab85-5e72fc64997a",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a210bc-6b09-7785-ab85-5e72fc64997a",
  "id": "01a210bd-9242-76cc-92d5-490b91a57fd4",
  "kind": "realization",
  "to_artifact_id": "01a210bc-b7e1-7a83-b322-81d74e0a903a",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a210bb-cfc9-72ec-85f5-619bf730fff9",
      "id": "01a210bd-7d79-74f8-b53b-e9e5a1b8bef6",
      "kind": "dependency",
      "to_artifact_id": "01a210bc-6b09-7785-ab85-5e72fc64997a",
      "version": 1
    },
    {
      "from_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
      "id": "01a210bd-6a41-7642-a2b5-fa70f7cf444c",
      "kind": "realization",
      "to_artifact_id": "01a210bc-6b09-7785-ab85-5e72fc64997a",
      "version": 1
    },
    {
      "from_artifact_id": "01a210bc-6b09-7785-ab85-5e72fc64997a",
      "id": "01a210bd-9242-76cc-92d5-490b91a57fd4",
      "kind": "realization",
      "to_artifact_id": "01a210bc-b7e1-7a83-b322-81d74e0a903a",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a210bc-92bd-71d7-964f-8d9761e78baa",
  "id": "01a210bd-a5ea-7f1e-abf9-ab00c0798585",
  "kind": "realization",
  "to_artifact_id": "01a210bc-b7e1-7a83-b322-81d74e0a903a",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a210bc-465a-7d83-b9c6-7c7b089d8b02",
      "id": "01a210bd-56bf-7cac-b247-ca534b179239",
      "kind": "realization",
      "to_artifact_id": "01a210bc-92bd-71d7-964f-8d9761e78baa",
      "version": 1
    },
    {
      "from_artifact_id": "01a210bc-92bd-71d7-964f-8d9761e78baa",
      "id": "01a210bd-a5ea-7f1e-abf9-ab00c0798585",
      "kind": "realization",
      "to_artifact_id": "01a210bc-b7e1-7a83-b322-81d74e0a903a",
      "version": 1
    }
  ]
}
```
