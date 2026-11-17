-- 0048_host_registry.sql — the hosts conveyor may reach, and what each may be asked for.
--
-- SPEC_V2 §10.4.1 fixes the mechanism: a host row names an addressable host and its role, a
-- per-host agent holds the host credential, the agent answers a forced-command verb set, and the
-- destructive effect class applies in full. This table is the row.
--
-- It is a PLATFORM fact, not tenant data: the estate's hosts are the estate's, and a job never
-- supplies a destination — the action names a row and the row supplies the address. No
-- `tenant_id` and no RLS, the `commercial_price` (0043/0045) precedent.
--
-- ⛔ NO SEEDED HOSTS. This repository is public: an estate address or an estate-internal DNS
-- name in a
-- migration trips `scripts/ci/no-private-refs.sh`, and a host row is deployment data, not schema.
-- A deployment inserts its rows (`scripts/host-configs/<host>/conveyor-host-agent/`), and an
-- unregistered host is refused by the action with a typed error rather than a guessed default.
--
-- `mechanism` is the OS surface the agent answers over: `launchctl` on the macOS fleet,
-- `systemctl` elsewhere. `units` is the allowlist — the agent refuses any unit not in it, and the
-- action refuses any unit not in the row, so the two refusals are independent.
--
-- `host_key` is the host's SSH public key, pinned. The client writes it to a per-call
-- `known_hosts` before it connects, so `StrictHostKeyChecking=yes` has an authority to check
-- against that came from the registry rather than from the network.
CREATE TABLE host_registry (
    -- The action's `host` input. A row a caller can name, never an address a caller can supply.
    name        text PRIMARY KEY CHECK (name ~ '^[a-z0-9][a-z0-9.-]{0,62}$'),
    -- The address the client connects to (an IP or a DNS name the tailnet resolves).
    address     text NOT NULL CHECK (address <> ''),
    -- What the host is for ("audio hosts", "the DGX") — operator documentation, never a dispatch.
    role        text NOT NULL DEFAULT '',
    mechanism   text NOT NULL CHECK (mechanism IN ('launchctl', 'systemctl')),
    -- The unprivileged account the forced-command key logs in as.
    agent_user  text NOT NULL CHECK (agent_user <> ''),
    -- The pinned host key, as it appears in `known_hosts` after the address.
    host_key    text NOT NULL CHECK (host_key <> ''),
    -- The units the agent may act on. Empty is legal and means "this row can do nothing" — never
    -- "any unit".
    units       text[] NOT NULL DEFAULT '{}',
    -- A disabled row is invisible to the actions; it is how a host is retired without a delete.
    enabled     boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE host_registry IS
    'The bare-metal hosts conveyor may reach (§10.4.1). Platform fact: no tenant_id, no RLS.';
COMMENT ON COLUMN host_registry.units IS
    'The unit allowlist the agent enforces. Empty means nothing may be asked, never anything.';
