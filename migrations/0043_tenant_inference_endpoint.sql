-- conveyor#160 P1 (SPEC_V2 §11.2, SPEC D31): the destination the executor calls for a tenant's
-- model requests. One row per tenant.
--
-- The credential is deliberately NOT here: it stays the sealed `inference.token` secret, so a
-- destination is ordinary configuration a console may show while the token remains
-- never-readable-back. The platform `CONVEYOR_EXTERNAL_INFERENCE_URL` remains the fallback for a
-- tenant with no row (P1.2) and is removed in P3 once the tenants that matter have registered.
--
-- The URL shape check is the migration's half of "an address, not a secret": http(s) only, no
-- whitespace. The executor still validates before use; this keeps a typo out of the table.

CREATE TABLE tenant_inference_endpoint (
    tenant_id       uuid PRIMARY KEY DEFAULT app_current_tenant(),
    destination_url text NOT NULL,
    updated_by      uuid REFERENCES app_user (id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tenant_inference_endpoint_tenant_fk
        FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT tenant_inference_endpoint_url_shape
        CHECK (destination_url ~ '^https?://[^[:space:]]+$')
);

ALTER TABLE tenant_inference_endpoint ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_inference_endpoint FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tenant_inference_endpoint FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
