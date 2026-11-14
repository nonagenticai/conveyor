#!/usr/bin/env bash
# The one normaliser, shared by capture-goldens.sh and verify.sh.
#
# Its contract is narrow on purpose: it removes only a value that is true of THIS run and false
# of the NEXT — a clock reading, a duration of the host, a generated id. It must never rewrite a
# value into the one a candidate happens to produce; a difference between two implementations
# belongs in tolerances.json, where it is named with a reason and can be argued with.

normalize() {
    jq -S '
      # Epoch clocks: the old service stamps "created" with time.time().
      if type == "object" then
        walk(
          if type == "object" and has("created") and (.created | type) == "number"
          then .created = 0 else . end
        )
      else . end
      # Durations and generated ids, wherever they appear.
      | walk(
          if type == "object" then
            with_entries(
              if (.key | test("(^|_)(latency_ms|duration_ms|elapsed_ms)$"))
              then .value = 0
              else . end
            )
          else . end
        )
      # Any RFC3339 timestamp, and any bare date (the timeline day bucket).
      | walk(
          if type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}")
          then "TIMESTAMP"
          elif type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
          then "DATE" else . end
        )
    ' 2>/dev/null || cat
}

# An SSE body is not JSON, so it is normalised frame by frame: the `data: ` prefix and the blank
# separator are framing and stay byte-for-byte, while each JSON frame goes through normalize().
# Capture and verify must use THIS function or a streamed case can never match.
normalize_sse() {
    sed 's/^data: //' | while IFS= read -r line; do
        if [ "$line" = "[DONE]" ] || [ -z "$line" ]; then
            printf '%s\n' "$line"
        else
            printf 'data: %s\n' "$(printf '%s' "$line" | normalize | jq -c . 2>/dev/null || printf '%s' "$line")"
        fi
    done
}
