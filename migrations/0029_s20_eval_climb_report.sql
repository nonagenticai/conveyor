-- 0029_s20_eval_climb_report.sql — the climb's final report (S20; SPEC D26).
--
-- `eval_climb` shipped complete in `0026_s19_eval.sql:306-333`, so S20 needs no table of its own:
-- a round is an `eval_run` row and the loop state is the column set 0026 already declares. The
-- one thing the table cannot express is the report — a status table plus the four-part summary
-- plus the named winner — because `eval_run.narrative_md` belongs to a *run*, and a climb that
-- ended without a winning round has no run to carry it.
--
-- CONTRACT-CHANGE REQUEST (S20, approved by the owner 2026-11-09): `02-DATA-ARCHITECTURE §2.20`
-- gains `eval_climb.narrative_md`. Additive and nullable: no existing row, query, index or view
-- changes, and a climb that never stops simply has none.
--
-- ⛔ There is still no `spend` column and one must not be added. Cumulative spend is summed from
-- the climb's `eval_case_result.usage` and the billed `usage` on `eval_attempt_error` rows at read
-- time; a maintained counter is the one thing that can drift from the rows it counts.

ALTER TABLE eval_climb
    ADD COLUMN narrative_md text;

COMMENT ON COLUMN eval_climb.narrative_md IS
    'The final report, written once by eval_climbs.stop: the per-round status table, the four-part summary and the named winner. NULL until the climb stops.';
