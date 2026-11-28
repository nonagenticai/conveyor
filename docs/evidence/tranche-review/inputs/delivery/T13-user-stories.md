# User Stories — T13 Tool platform

## Personas

- **The development job** calls coding through the broker, never an ad-hoc backend call.
- **The tool operator** binds a tool to a destination, declares its effect, and reads the call
  ledger.
- **The tenant administrator** holds the approval configuration a destructive call needs.
- **The auditor** reads one `tool_call` row per call and the redacted payload.

## Stories

- As a development job I call coding through the broker and get the destination's answer.
- As a tool operator I declare a tool's effect and binding once; an undeclared or mis-bound verb
  is refused.
- As the tenant administrator no destructive call runs without the tenant's configured
  approvals.
- As the auditor every call is one ledger row with a 64-hex input hash and a redacted payload.
- As a job I write and read a tenant-prefixed object without an S3 key in my prompt.

## Scenarios

- Given a bound job, when it calls coding through the broker, then the destination is reached by
  the declared path and no ad-hoc backend call is made.
- Given a destructive tool, when it is called without the configured approvals, then the call is
  refused (403 `approval_required`) with zero destination hits and no ledger row.
- Given schema-invalid input, when the tool is called, then it is refused (422
  `tool_input_invalid`) before the backend call.
- Given a valid call, when it completes, then exactly one `tool_call` row and one `tool.called`
  event are written.
- Given a job object, when it is written and read back, then the key is tenant-prefixed and the
  hash verifies.

## Verify

- `crates/actions/tests/tool_platform.rs`: the seven scenarios, including the counted stub of the
  declared `coding_jobs.create` destination.
- `crates/actions/tests/s3_wire.rs`: the wire-level storage round trip.
- `crates/actions/tests/parity_http_mcp.rs`: the 409 parity.
- `acceptance/programme/T13.toml`: the manifest's checks against the live rows.
