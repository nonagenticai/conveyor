# T19F live gate — the cadence row

Action `cadences.create`, period `2026-PI-T19F`, bound to `t19f_live_gate_forum`.


```
== write: cadences.create ==
{
  "id": "01a210c9-0d47-7473-8339-7ea7648e59e9",
  "kind": "pi",
  "meeting_type_key": "t19f_live_gate_forum",
  "meta": {},
  "period": "2026-PI-T19F",
  "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
  "state": "planned",
  "version": 1
}
== read-back: cadences.list ==
{
  "items": [
    {
      "id": "01a1ff21-33f0-7e5a-bafc-b09b6fc73c13",
      "kind": "pi",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-ced2-7769-b12f-f6fba0997914",
        "tranche_number": 0
      },
      "period": "tranche-0",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a1ff21-547f-73e8-b256-4bee7fdc9c70",
      "kind": "refinement",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-ced2-7769-b12f-f6fba0997914",
        "tranche_number": 0
      },
      "period": "tranche-0",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a1ff21-7354-7777-996b-4c8cb12f25e9",
      "kind": "demo",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-ced2-7769-b12f-f6fba0997914",
        "tranche_number": 0
      },
      "period": "tranche-0",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "materialised",
      "version": 2
    },
    {
      "id": "01a1ff21-8b3b-7d8c-a94f-7ea0e53dbe66",
      "kind": "retro",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-ced2-7769-b12f-f6fba0997914",
        "tranche_number": 0
      },
      "period": "tranche-0",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a1ff73-df1f-78c5-8793-101de5a3dd60",
      "kind": "pi",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-d787-7f5c-b5ed-15b05f3dc531",
        "tranche_number": 1
      },
      "period": "tranche-1",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 2
    },
    {
      "id": "01a1ff74-8982-77b9-971e-e4517c55d09a",
      "kind": "refinement",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-d787-7f5c-b5ed-15b05f3dc531",
        "tranche_number": 1
      },
      "period": "tranche-1",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a1ff74-9819-72dc-a1f2-e102c73e9672",
      "kind": "demo",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-d787-7f5c-b5ed-15b05f3dc531",
        "tranche_number": 1
      },
      "period": "tranche-1",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "materialised",
      "version": 2
    },
    {
      "id": "01a1ff74-a4d5-773a-a565-4bb5f9153557",
      "kind": "retro",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-d787-7f5c-b5ed-15b05f3dc531",
        "tranche_number": 1
      },
      "period": "tranche-1",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a2004a-51f4-7482-b5f8-455a4bd371b5",
      "kind": "pi",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-e2ac-786b-ac48-3b8bcdf12188",
        "tranche_number": 2
      },
      "period": "tranche-2",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a2004a-739d-717e-81dd-15ddf37f4c2f",
      "kind": "refinement",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-e2ac-786b-ac48-3b8bcdf12188",
        "tranche_number": 2
      },
      "period": "tranche-2",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a2004a-87d8-77e9-b06a-171361548e38",
      "kind": "demo",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-e2ac-786b-ac48-3b8bcdf12188",
        "tranche_number": 2
      },
      "period": "tranche-2",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "materialised",
      "version": 2
    },
    {
      "id": "01a2004a-9c9a-7999-bbbd-9976427009b6",
      "kind": "retro",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "retrospective": true,
        "tranche_id": "01a1e9e0-e2ac-786b-ac48-3b8bcdf12188",
        "tranche_number": 2
      },
      "period": "tranche-2",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a20280-a739-7bb9-823f-232f34b32aaa",
      "kind": "pi",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "tranche_id": "01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52",
        "tranche_number": 3
      },
      "period": "tranche-3",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a20280-d52b-7791-a9b1-ebc2a10b5c3d",
      "kind": "refinement",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "tranche_id": "01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52",
        "tranche_number": 3
      },
      "period": "tranche-3",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a20280-ef3f-7aac-8fad-3840a7995847",
      "kind": "demo",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "tranche_id": "01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52",
        "tranche_number": 3
      },
      "period": "tranche-3",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "materialised",
      "version": 2
    },
    {
      "id": "01a20281-09ba-709a-b61e-51a22f2ed172",
      "kind": "retro",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "tranche_id": "01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52",
        "tranche_number": 3
      },
      "period": "tranche-3",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a202a1-dbb9-7e49-b64a-cf97c3fab095",
      "kind": "pi",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "tranche_id": "01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce",
        "tranche_number": 4
      },
      "period": "tranche-4",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a202a1-f4ee-7acc-8e57-50ac6f0417fd",
      "kind": "demo",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "tranche_id": "01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce",
        "tranche_number": 4
      },
      "period": "tranche-4",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "materialised",
      "version": 2
    },
    {
      "id": "01a202a2-0e3f-75be-9f2b-c572ddcdfce9",
      "kind": "retro",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "tranche_id": "01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce",
        "tranche_number": 4
      },
      "period": "tranche-4",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a202a2-2705-7392-a2d1-03ca2b3a174e",
      "kind": "refinement",
      "meeting_type_key": null,
      "meta": {
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "tranche_id": "01a1e9e0-f4c0-727d-9afb-bb5b1e9783ce",
        "tranche_number": 4
      },
      "period": "tranche-4",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    },
    {
      "id": "01a210c9-0d47-7473-8339-7ea7648e59e9",
      "kind": "pi",
      "meeting_type_key": "t19f_live_gate_forum",
      "meta": {},
      "period": "2026-PI-T19F",
      "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
      "state": "planned",
      "version": 1
    }
  ]
}
```
