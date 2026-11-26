-- #760: the conveyor's channel client maps to owner `agent`.
--
-- The session channel mints an opaque licence bearer through `POST /licenses/{id}/tokens`.
-- That route authorizes on two facts about the caller: its `api_client` owner must be an issuer
-- of the licence's app (`app_issuer` for app `conveyor` is {agent, fr}), and that owner must own
-- or be granted the tenant (`tenant_app_grant`). The dedicated secure-apps client
-- `conveyor-channel` exists for exactly this one act; mapping it to `agent` keeps the mint
-- scoped to the two apps agent issues {agent, conveyor} instead of the twelve fr issues.
--
-- Idempotent, and additive only: an existing mapping is never rewritten (a different owner is a
-- deliberate operational state this migration must not overwrite).
INSERT INTO api_client (realm, client_id, owner_key, cross_owner)
VALUES ('secure-apps', 'conveyor-channel', 'agent', false)
ON CONFLICT (realm, client_id) DO NOTHING;
