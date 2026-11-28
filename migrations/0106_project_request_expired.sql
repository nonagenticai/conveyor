-- 0106_project_request_expired.sql — S07 (v1-s07-1): `project_request.timeout`.
--
-- S06 §5.3 maps fr's `project-request-analysis-timeout` timer to the `project_request.timeout`
-- job; S07 fixes the window at 24 h. The sweep expires a `draft` intake **in place**: the row
-- keeps its payload, external_ref and project_id (NULL until promote) and remains readable —
-- nothing is deleted. `expired` is terminal: `approve`/`reject`/`promote` refuse it and a
-- re-post answers the `Locked` 409, exactly as an `approved` row does.
--
-- Expand only: the widened CHECK still admits every value the prior release writes, so an
-- application-only rollback serves the older binary against this schema unchanged.

ALTER TABLE project_request
    DROP CONSTRAINT project_request_status_check,
    ADD CONSTRAINT project_request_status_check
        CHECK (status IN ('draft', 'approved', 'rejected', 'promoted', 'expired'));
