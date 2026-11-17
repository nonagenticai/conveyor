-- 0049_approval_policies.sql — how a tenant wants its destructive actions gated.
--
-- SPEC_V2 §11.4: `destructive` = approval + dry-run + dual control. §5.4: a requester cannot
-- decide the gate that judges their own work. #282 moves both from code into data: the gate is
-- enforced once at the invoke boundary, and the two decisions a tenant can make about it live
-- here — whether an action needs an approval at all, and whether its requester may decide it.
--
-- ⛔ This is a TENANT table, not a platform fact: two tenants may disagree about the same action,
-- and the RLS policy below is the same one every other tenant table carries.
--
-- ⛔ A MISSING ROW IS THE SAFE DEFAULT, NOT "no policy". `required` + `forbidden` are what §11.4
-- and §5.4 demand, so the reader treats an absent row exactly like that pair. A migration that
-- seeded rows would turn "the tenant has not decided" into "the tenant decided", and a tenant
-- could then read a waiver it never granted.
--
-- `action` is the action's own name (`ops.restart`), which is also what `approval.gate` carries
-- for a destructive gate — that equality is what lets the decide path read this policy for the
-- gate it is about to resolve, without a caller ever naming the action again.
--
-- The policy does NOT widen an effect ceiling and cannot grant a tool: it only ever lowers the
-- approval requirement for ONE named action, and `approval_policies.set` is itself audited like
-- every other write.
CREATE TABLE approval_policy (
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    -- The action this policy governs. A name no registry action carries is refused by the
    -- action layer, not by the database (the registry is not a table).
    action        text NOT NULL,
    -- `required` — the default and the §11.4 rule; `waived` — this tenant's deliberate opt-out.
    approval      text NOT NULL DEFAULT 'required',
    -- `forbidden` — the default and the §5.4 rule; `allowed` — this tenant's deliberate opt-out.
    self_approval text NOT NULL DEFAULT 'forbidden',
    updated_at    timestamptz NOT NULL DEFAULT now(),
    updated_by    uuid,
    CONSTRAINT approval_policy_pkey PRIMARY KEY (tenant_id, action),
    CONSTRAINT approval_policy_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT approval_policy_action_format CHECK (action ~ '^[a-z][a-z0-9_.-]{0,63}$'),
    CONSTRAINT approval_policy_approval_vocabulary
        CHECK (approval IN ('required', 'waived')),
    CONSTRAINT approval_policy_self_vocabulary
        CHECK (self_approval IN ('forbidden', 'allowed'))
);

ALTER TABLE approval_policy ENABLE ROW LEVEL SECURITY;
ALTER TABLE approval_policy FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON approval_policy FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
