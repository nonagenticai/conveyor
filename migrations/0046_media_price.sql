-- 0046_media_price.sql — the non-token GPU surfaces get a unit and a per-unit price.
--
-- 0045 priced tokens. The engine also writes a ledger row for the external media actions
-- (`media.generate_image`, `produce.speech`, `media.transcribe`), but those responses carry no
-- `usage` object, so the row has 0 tokens and the token formula can never price it. This migration
-- adds the commercial unit and its rate to the platform catalogue, and the call's own unit count to
-- the ledger.
--
-- ⛔ A media call whose unit the engine cannot measure (a non-WAV transcription upload) is UNPRICED,
-- never $0. The report counts it separately, exactly as 0045 does for a model with no price row.
--
-- The catalogue stays a PLATFORM fact: no `tenant_id`, no RLS. The seeds are OpenAI's public list
-- at 2026-11-17; an operator edits a row to change the basis, never the ledger history.
ALTER TABLE commercial_price
    ADD COLUMN unit text NOT NULL DEFAULT 'token'
        CHECK (unit IN ('token', 'image', 'character', 'second')),
    ADD COLUMN unit_usd numeric(12,6)
        CHECK (unit_usd IS NULL OR unit_usd >= 0),
    -- A token row prices tokens and has no unit rate; a non-token row is the inverse.
    ADD CONSTRAINT commercial_price_unit_shape
        CHECK ((unit = 'token') = (unit_usd IS NULL));

ALTER TABLE inference_call
    ADD COLUMN usage_unit text
        CHECK (usage_unit IS NULL OR usage_unit IN ('image', 'character', 'second')),
    ADD COLUMN usage_units numeric(18,6)
        CHECK (usage_units IS NULL OR usage_units >= 0),
    -- A count without a kind cannot be priced, and a kind without a count is not a measurement.
    ADD CONSTRAINT inference_call_usage_pair
        CHECK (usage_units IS NULL OR usage_unit IS NOT NULL);

INSERT INTO commercial_price
    (model, equivalent, input_usd_per_1m, output_usd_per_1m, unit, unit_usd, source)
VALUES
    ('image_gen', 'gpt-image-1', 0, 0, 'image', 0.040000,
     'OpenAI public price list (gpt-image-1, 1024x1024, medium), 2026-11-17'),
    ('tts', 'tts-1', 0, 0, 'character', 0.000015,
     'OpenAI public price list (tts-1, 15 USD per 1M characters), 2026-11-17'),
    ('asr', 'whisper-1', 0, 0, 'second', 0.000100,
     'OpenAI public price list (whisper-1, 0.006 USD per minute), 2026-11-17');
