-- inference_call.traffic_source — benchmark vs live, stamped at write time (alembic 0012).
-- NOT VALID binds every new row without scanning the table under the ALTER's lock; existing rows
-- are NULL, which the check admits (NULL is "unknown", never "live").
SET LOCAL lock_timeout = '5s';

ALTER TABLE inference_call ADD COLUMN traffic_source text;

ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_traffic_source
    CHECK (traffic_source IN ('benchmark', 'live')) NOT VALID;
