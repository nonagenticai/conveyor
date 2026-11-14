#!/usr/bin/env bash
# The one normaliser, shared by verify.sh and shadow.sh.
#
# Its contract is narrow on purpose: it removes only a value that is true of THIS run and false
# of the NEXT — a clock reading, a generated secret. It must never rewrite a value into the one a
# candidate happens to produce; a difference between two implementations belongs in
# tolerances.json, where it is named with a reason and can be argued with.
#
# UUIDs are NOT masked: every id in the synthetic seed is deterministic, so a differing id is a
# real difference. Tokens ARE masked: the mint path generates a fresh secret every call.

normalize() {
    jq -S '
      # Any RFC3339 timestamp (created_at, expires_at, last_used_at, ...), and any bare date.
      walk(
        if type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}")
        then "TIMESTAMP"
        elif type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
        then "DATE" else . end
      )
      # A minted secret is a fresh value on every call; it is not part of the shape of the
      # contract.
      | walk(
          if type == "object" then
            with_entries(
              if (.key == "token") and (.value | type) == "string" then .value = "TOKEN"
              else . end
            )
          else . end
        )
    ' 2>/dev/null || cat
}
