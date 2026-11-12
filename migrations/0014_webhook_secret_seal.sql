-- 0014_webhook_seal.sql — seal the webhook signing secret under the S09 KEK.
--
-- `webhook_subscription.secret_ciphertext` (0004) has always held the bearer token in the clear; the column
-- name was forward-looking and the store's header said so. S09 shipped the envelope (`secret::Sealer`:
-- HKDF-SHA256(KEK, tenant) → XChaCha20-Poly1305), so the token can now be sealed like every other secret.
--
-- `secret_key_id` is nullable on purpose: NULL marks a row written before this migration, which the reader
-- still delivers with and `conveyor admin reseal-webhooks` converts. A NOT NULL column would need every
-- deployment to run the reseal before the new binary could read a single subscription.

ALTER TABLE webhook_subscription
    ADD COLUMN secret_key_id text;

COMMENT ON COLUMN webhook_subscription.secret_ciphertext IS
    'Sealed nonce||ciphertext||tag when secret_key_id is set; the legacy plaintext bearer token when NULL.';

COMMENT ON COLUMN webhook_subscription.secret_key_id IS
    'The KEK generation that sealed the row (`kek:v1`). NULL = written before 0014, awaiting reseal.';

-- Finding the rows the reseal still has to convert, without scanning sealed ones.
CREATE INDEX webhook_subscription_unsealed_idx
    ON webhook_subscription (tenant_id)
    WHERE secret_ciphertext IS NOT NULL AND secret_key_id IS NULL;
