# T19 close-out — live write evidence

Every write listed here is invoked through `scripts/programme-record.py` against the deployed
engine (`a8f67c3e`, tenant `nonagentic`), with a read-back action and a non-zero exit on an
unverified claim. The write outputs are captured beside this file under `docs/evidence/t19/`.

The human seat is exercised by the tenant's admin identity `FR Test`
(`app_user 01a1cf1e-09dd-7580-99cc-de395f3e86cc`), supplied by the owner for this live proof. The
evidence records that identity exactly; it does not claim the owner's personal account.

Writes: `approvals.request` ×3 (two deciding items, one minutes gate), `meetings.schedule`,
`meetings.open`, `meetings.invite`, `meetings.agenda_add` ×2, `meetings.minutes_publish`,
`meetings.close`. The owner's positions and the minutes decision are taken in the signed-in browser
(web room) and through the MCP transport, not through this recorder.
