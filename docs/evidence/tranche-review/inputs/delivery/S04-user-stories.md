# User Stories — S04 Action registry

## Personas

- **The action author** declares an action once and gets HTTP, MCP, OpenAPI, the admin CLI and
  external ingest from that one declaration.
- **The transport client** calls the served spec and gets the same schema, scopes and errors on
  every transport.
- **The operator** reduces the allowlist from one registry and reads one audit path.
- **The external integrator** ingests through the same fold the platform uses.

## Stories

- As an action author I declare an ActionSpec once and never fork it per transport; a second
  declaration path is a defect.
- As a transport client I receive the same served operation and the same typed refusal on HTTP
  and MCP.
- As the operator I read one registry (549 actions) and one served spec locked by hash.
- As an external integrator I ingest through the declared fold and read the created rows back.

## Scenarios

- Given a new action declaration, when the derive crate generates the spec, then the action is
  registered with its scopes, effect and approval target.
- Given the same action, when HTTP and MCP invoke it, then both answer with the same result and
  the same error shape.
- Given a changed spec, when the lock check runs, then the served document is regenerated and
  the new hash recorded, and a drift fails the check.
- Given an external ingest payload, when it is folded, then the declared rows exist and the fold
  is idempotent by its natural key.

## Verify

- `acceptance/programme/S04.toml`: the manifest's checks (derive crate, registry and
  vocabularies, parity, lock, ingest) execute against the live rows.
- `crates/actions/tests/parity_http_mcp.rs`: HTTP/MCP parity of results and refusals.
- The served-spec lock check in CI (`conveyor snapshot --check-lock`).
