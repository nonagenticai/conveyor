# Delivery Contract — Documents, repositories, secrets, workers UI, reports/metrics, billing display

## Commitments

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a219da-0ec0-706f-9949-eb2bc03f2fdf",
      "field": "commitments",
      "kind": "contract_field",
      "title": "commitments: S09 commits to the document, repository, secret and read-only display boundary of the delivery track: four tables, the integrations module, 26 of the front matter's 27 actions, nine event types, document bytes in MinIO behind an authorized stream, write-only secrets, repositories as references, reports and metrics that match their SQL counts, and the nine adjacent displays, with a 1 MiB upload round-tripping to an equal sha256.",
      "value": "S09 commits to the document, repository, secret and read-only display boundary of the delivery track: four tables, the integrations module, 26 of the front matter's 27 actions, nine event types, document bytes in MinIO behind an authorized stream, write-only secrets, repositories as references, reports and metrics that match their SQL counts, and the nine adjacent displays, with a 1 MiB upload round-tripping to an equal sha256."
    }
  ],
  "source": "computed:contract_field"
}
```

## Compliance method

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a219da-0ec0-706f-9949-eb2bc03f2fdf",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: S09 is assessed clause by clause against acceptance/programme/S09.toml and the plan section 7 gate. The seven properties are: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, delete yielding 404 and object_store::head NotFound; (b) cap \u2014 10,485,760 bytes accepted and 10,485,761 refused 413; (c) secret \u2014 rotation to version 2 with no plaintext, value key or ciphertext in the response; (d) workers \u2014 live rows plus a stopped-service banner and the breaker open after five failures; (e) reports \u2014 every number equal to its SQL count on the same snapshot; (f) externals \u2014 timeout_ms on every external and zero reqwest:: outside the executor; (g) billing \u2014 no write action and no metering event. The instrument also requires migration 0013 twice, FORCE RLS on the four tables, 26 of 27 actions registered and the nine event types. The snapshot records no run of these properties; this rework supplies the lifecycle record and records the gap.",
      "value": "S09 is assessed clause by clause against acceptance/programme/S09.toml and the plan section 7 gate. The seven properties are: (a) round-trip \u2014 a 1 MiB upload, list and download with sha256 equal to content_hash, delete yielding 404 and object_store::head NotFound; (b) cap \u2014 10,485,760 bytes accepted and 10,485,761 refused 413; (c) secret \u2014 rotation to version 2 with no plaintext, value key or ciphertext in the response; (d) workers \u2014 live rows plus a stopped-service banner and the breaker open after five failures; (e) reports \u2014 every number equal to its SQL count on the same snapshot; (f) externals \u2014 timeout_ms on every external and zero reqwest:: outside the executor; (g) billing \u2014 no write action and no metering event. The instrument also requires migration 0013 twice, FORCE RLS on the four tables, 26 of 27 actions registered and the nine event types. The snapshot records no run of these properties; this rework supplies the lifecycle record and records the gap."
    }
  ],
  "source": "computed:contract_field"
}
```

## Approval

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_at": "2026-11-26T21:54:11.051763+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a219da-0f63-7563-b913-3abcd4ed1979",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a219da-0f63-7563-b913-3abcd4ed1979 \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```
