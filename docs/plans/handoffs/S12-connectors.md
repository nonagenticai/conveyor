# S12 handoff — Connectors and the interaction inbox (S06d)

The backend is built. This is what the web UI needs to consume; the actions are live and their
payloads below are exact.

## New screen: `apps/web/src/screens/connectors.rs`, route `/settings/connectors`

Nav entry beside **Roles & permissions**, gated on `ctx.has_action("channels.kinds")`
(`channels:write` — admin only; member/viewer get `403`).

### 1. Connector list

`channels.list` → `{ bindings: [ChannelBindingAdminOut] }`. One row per binding:

| field | render |
|---|---|
| `channel_kind` | the kind's `label` from `channels.kinds` (not the raw kind) |
| `route_key` | route selector |
| `active` | status pill |
| `inbound_url` | copy button (absolute = page origin + path) for the kinds that receive |
| `secrets_set` | `{secretName: bool}` — show ✓/✗, **never a value** |
| `config` | non-secret values only (chat id, endpoint URL, …) |

Row actions: **Test connection** → `channels.test {binding_id}` (queues a real delivery; show
“queued — delivery <id>”), **Edit**, **Disable/Remove** → `channels.unbind {tenant_id,
channel_kind, route_key}`.

### 2. Add/edit connector

1. `channels.kinds` → `{ kinds: [{kind,label,can_send,can_receive,identity,config_schema,
   secret_fields,default_actions,instructions}] }`.
2. Pick a kind → render `config_schema` as a form (generic JSON-Schema renderer; object with
   properties, `required`, types). `secret_fields` are **secret names**, not values: each gets a
   text input plus an inline “create secret” flow (`secrets.create {name, value}` — write-only,
   never echoed). `instructions` is the operator copy (Telegram: BotFather + `setWebhook` with
   the binding's `inbound_url`; webhook: the POST envelope).
3. Save → `channels.bind {tenant_id, channel_kind, route_key, config}`. The response is the
   admin binding: show `inbound_url` and `secrets_set`; a 422 names the field and, for a missing
   secret, says to create it first.
4. `identity` values: `provisioned` (the backend creates the tenant service account),
   `credential` (MCP — no config; status is the licence principal), `human` (web — link to
   Roles & permissions; no machine matrix).

### 3. Authorization matrix

`channels.policy_get` → `{ channels: [ChannelPolicyChannel] }`, one card per **machine** kind
(telegram, webhook, mcp; never web). Each card: `label`, `bound`, `bindings[]`, `identity`,
`groups[]`; each group is `{resource, actions: [{action, default, allowed, overridden,
grantable, denial}]}`.

- Render rows grouped by `resource`, sorted.
- `allowed` drives the toggle; `default` gets a “default” chip; `overridden` gets “custom”.
- `grantable: false` rows are disabled and show `denial` (“human sign-off”, “person-only”,
  “administration”, “workflow authoring”, “platform”).
- Save → `channels.policy_set {channel_kind, entries: [{action, allowed}]}` (send the complete
  override set for the card; omitted = compiled default). Reset → `channels.policy_reset
  {channel_kind}`. Render the response body; no optimistic state.
- Copy: “Telegram, webhook and MCP act as this tenant's machine identities. They can never sign
  off evals, read held-out traces, administer members/roles/invites/departments/channels/webhooks/secrets/licences, control board membership, or change these
  permissions.”

### 4. Interaction inbox / approval detail

- The caller's own inbox is already `notifications.list` (one row per delivery, with `state` and
  `last_error`). Use it for the “my requests” view. **The admin-wide dispatch ledger is not yet
  exposed as an action** — an approval detail can say “offered to Telegram + web” only when the
  backend adds a read surface; do not invent one in the client. Flag it to this session as a
  follow-up if the screen needs it.
- Terminal delivery states to render: `delivered`, `failed`, and **`expired` with
  `last_error` starting `superseded: resolved via `** → “answered via <channel>”.
- Ordinary gates answer with `approvals.decide`; **eval gates keep `eval_signoffs.decide`**
  (admin-only) and must not show a machine-channel answer control.
- Approval history: render `via.channel` and `via.sender_ref` (Telegram `tg:<id>`, webhook
  `mailto:…`) so a connector decision is attributable.

## Copy/paste action inputs

```json
channels.bind      {"tenant_id":"…","channel_kind":"telegram","route_key":"default",
                    "config":{"chat_id":"-100…","topic_id":null,
                              "bot_token_secret":"telegram-token",
                              "incoming_secret":"telegram-incoming"}}
channels.test      {"binding_id":"…"}
channels.policy_set{"channel_kind":"telegram","entries":[{"action":"cards.create","allowed":true}]}
channels.policy_reset {"channel_kind":"telegram"}
```

## States to handle

- 403 for a non-admin (the nav item is hidden by `has_action`, but the route can be typed).
- Empty: “No connectors yet — add Telegram or a webhook.” Web and MCP cards still render.
- 422 on bind: per-field `errors[]`, including `secret_not_found`.
- Test connection is asynchronous: the delivery is queued; poll the delivery row or show the id.
