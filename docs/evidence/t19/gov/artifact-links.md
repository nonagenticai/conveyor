# T19 artefact link — driver_goal_objective derivation requirements_catalog

Action `architecture.artifacts_link` from 01a20cde-0f97-7f74-afd8-7cd74e0990e3 to 01a20cde-371c-7d85-ae2d-780453958bef, kind `derivation` (docs/plans/T19/architecture-design.md §5).


```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a20cde-0f97-7f74-afd8-7cd74e0990e3",
  "id": "01a20cdf-cacf-744d-9147-64e4091a3a47",
  "kind": "derivation",
  "to_artifact_id": "01a20cde-371c-7d85-ae2d-780453958bef",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a20cde-0f97-7f74-afd8-7cd74e0990e3",
      "id": "01a20cdf-cacf-744d-9147-64e4091a3a47",
      "kind": "derivation",
      "to_artifact_id": "01a20cde-371c-7d85-ae2d-780453958bef",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a20cde-371c-7d85-ae2d-780453958bef",
  "id": "01a20cdf-de08-7ce2-8d47-1dd19702d147",
  "kind": "derivation",
  "to_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a20cde-0f97-7f74-afd8-7cd74e0990e3",
      "id": "01a20cdf-cacf-744d-9147-64e4091a3a47",
      "kind": "derivation",
      "to_artifact_id": "01a20cde-371c-7d85-ae2d-780453958bef",
      "version": 1
    },
    {
      "from_artifact_id": "01a20cde-371c-7d85-ae2d-780453958bef",
      "id": "01a20cdf-de08-7ce2-8d47-1dd19702d147",
      "kind": "derivation",
      "to_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
  "id": "01a20cdf-f295-76b4-b9eb-1465f0484504",
  "kind": "realization",
  "to_artifact_id": "01a20cde-adbf-7626-9b2b-58faf7164755",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a20cde-371c-7d85-ae2d-780453958bef",
      "id": "01a20cdf-de08-7ce2-8d47-1dd19702d147",
      "kind": "derivation",
      "to_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
      "version": 1
    },
    {
      "from_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
      "id": "01a20cdf-f295-76b4-b9eb-1465f0484504",
      "kind": "realization",
      "to_artifact_id": "01a20cde-adbf-7626-9b2b-58faf7164755",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
  "id": "01a20ce0-0567-7c74-be0f-7eb958c4b0f5",
  "kind": "realization",
  "to_artifact_id": "01a20cde-86d7-7257-a45e-b4731ab423a4",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a20cde-371c-7d85-ae2d-780453958bef",
      "id": "01a20cdf-de08-7ce2-8d47-1dd19702d147",
      "kind": "derivation",
      "to_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
      "version": 1
    },
    {
      "from_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
      "id": "01a20cdf-f295-76b4-b9eb-1465f0484504",
      "kind": "realization",
      "to_artifact_id": "01a20cde-adbf-7626-9b2b-58faf7164755",
      "version": 1
    },
    {
      "from_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
      "id": "01a20ce0-0567-7c74-be0f-7eb958c4b0f5",
      "kind": "realization",
      "to_artifact_id": "01a20cde-86d7-7257-a45e-b4731ab423a4",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a20cdd-e71f-7d06-8ca5-701ff6b7d69e",
  "id": "01a20ce0-1850-7c77-ab61-acf6c28b9f9c",
  "kind": "dependency",
  "to_artifact_id": "01a20cde-86d7-7257-a45e-b4731ab423a4",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a20cdd-e71f-7d06-8ca5-701ff6b7d69e",
      "id": "01a20ce0-1850-7c77-ab61-acf6c28b9f9c",
      "kind": "dependency",
      "to_artifact_id": "01a20cde-86d7-7257-a45e-b4731ab423a4",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a20cde-86d7-7257-a45e-b4731ab423a4",
  "id": "01a20ce0-2bcc-734b-a713-c5e0fefa98b8",
  "kind": "realization",
  "to_artifact_id": "01a20cde-d6bd-7cfe-88ad-42a1e6db30a7",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a20cdd-e71f-7d06-8ca5-701ff6b7d69e",
      "id": "01a20ce0-1850-7c77-ab61-acf6c28b9f9c",
      "kind": "dependency",
      "to_artifact_id": "01a20cde-86d7-7257-a45e-b4731ab423a4",
      "version": 1
    },
    {
      "from_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
      "id": "01a20ce0-0567-7c74-be0f-7eb958c4b0f5",
      "kind": "realization",
      "to_artifact_id": "01a20cde-86d7-7257-a45e-b4731ab423a4",
      "version": 1
    },
    {
      "from_artifact_id": "01a20cde-86d7-7257-a45e-b4731ab423a4",
      "id": "01a20ce0-2bcc-734b-a713-c5e0fefa98b8",
      "kind": "realization",
      "to_artifact_id": "01a20cde-d6bd-7cfe-88ad-42a1e6db30a7",
      "version": 1
    }
  ]
}
```

```
== write: architecture.artifacts_link ==
{
  "from_artifact_id": "01a20cde-adbf-7626-9b2b-58faf7164755",
  "id": "01a20ce0-3ebe-76bb-a6da-63d71cacf574",
  "kind": "realization",
  "to_artifact_id": "01a20cde-d6bd-7cfe-88ad-42a1e6db30a7",
  "version": 1
}
== read-back: architecture.artifacts_links ==
{
  "items": [
    {
      "from_artifact_id": "01a20cde-5f86-7720-9a16-36a964ef255d",
      "id": "01a20cdf-f295-76b4-b9eb-1465f0484504",
      "kind": "realization",
      "to_artifact_id": "01a20cde-adbf-7626-9b2b-58faf7164755",
      "version": 1
    },
    {
      "from_artifact_id": "01a20cde-adbf-7626-9b2b-58faf7164755",
      "id": "01a20ce0-3ebe-76bb-a6da-63d71cacf574",
      "kind": "realization",
      "to_artifact_id": "01a20cde-d6bd-7cfe-88ad-42a1e6db30a7",
      "version": 1
    }
  ]
}
```
