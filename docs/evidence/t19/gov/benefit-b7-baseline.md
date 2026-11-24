# B7 — the measured baseline

Action `benefits.update` for benefit profile 01a20c80-c0c3-7788-9cc2-bb18a102a2f7 (B7), setting the pre-T19 baseline to a measurement record: zero deliberated cross-department decision rows existed before T19 (docs/plans/T19/inception-analysis.md). The act is delegated by the standing delegation 01a1fb9f-ac2d-732d-b274-1079be3dd2b3.


```
== write: benefits.update ==
{
  "baseline": {
    "measured_at": "2026-11-24",
    "method": "count of meeting rows with a closed deciding agenda item before T19; the pre-T19 record holds approval rows only (docs/plans/T19/inception-analysis.md)",
    "unit": "deliberated cross-department decision rows before T19",
    "value": 0
  },
  "benefit_owner_job_id": "01a1db6b-4f17-7e76-838d-590d71578ad1",
  "capabilities": [],
  "category": "programme benefit",
  "change_ref": null,
  "cost": {},
  "description": "Deliberated cross-department decisions",
  "financial_value": "",
  "id": "01a20c80-c0c3-7788-9cc2-bb18a102a2f7",
  "kpis": [],
  "measurement_method": "Read-time aggregates over meeting, meeting_agenda_item, meeting_attendance, meeting_position, approval_decision and project_dependency; a metric reads rows and writes nothing.",
  "objectives": [],
  "observable_outcome": "",
  "operational_area": "BCM",
  "owner_job_id": "01a1db6b-4f17-7e76-838d-590d71578ad1",
  "preconditions": [],
  "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
  "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
  "reference": "B7",
  "retired_reason": "",
  "risks": [],
  "status": "draft",
  "target": "every tranche-4 cross-department decision has a meeting record with a position per required department; quorum-miss and commitment slippage are reported at the review",
  "version": 2
}
== read-back: benefits.get ==
{
  "baseline": {
    "measured_at": "2026-11-24",
    "method": "count of meeting rows with a closed deciding agenda item before T19; the pre-T19 record holds approval rows only (docs/plans/T19/inception-analysis.md)",
    "unit": "deliberated cross-department decision rows before T19",
    "value": 0
  },
  "benefit_owner_job_id": "01a1db6b-4f17-7e76-838d-590d71578ad1",
  "capabilities": [],
  "category": "programme benefit",
  "change_ref": null,
  "cost": {},
  "description": "Deliberated cross-department decisions",
  "financial_value": "",
  "id": "01a20c80-c0c3-7788-9cc2-bb18a102a2f7",
  "kpis": [],
  "measurement_method": "Read-time aggregates over meeting, meeting_agenda_item, meeting_attendance, meeting_position, approval_decision and project_dependency; a metric reads rows and writes nothing.",
  "objectives": [],
  "observable_outcome": "",
  "operational_area": "BCM",
  "owner_job_id": "01a1db6b-4f17-7e76-838d-590d71578ad1",
  "preconditions": [],
  "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
  "project_id": "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1",
  "reference": "B7",
  "retired_reason": "",
  "risks": [],
  "status": "draft",
  "target": "every tranche-4 cross-department decision has a meeting record with a position per required department; quorum-miss and commitment slippage are reported at the review",
  "version": 2
}
```
