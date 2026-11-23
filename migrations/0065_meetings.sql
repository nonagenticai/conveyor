-- 0065_meetings.sql — T19: decision forums (meetings).
--
-- SPEC_V2 §6.8, §8.1–§8.3, §10.3–§10.5, §12 T19, D66; the slice plan is
-- docs/plans/T19-decision-forums.md.
--
-- The meeting is a card plus a typed record: agenda items resolve existing approvals, positions
-- are the deliberation, and close_item derives the outcome by arithmetic over those positions and
-- the linked approval's stricter policy. No table here is a second decision store: a meeting
-- decision is an `approval_decision` row through the participant extension at the bottom.
--
-- `meeting_type` is versioned: a meeting pins `meeting_type_id` and `meeting_type_version`, so a
-- policy change never rewrites an in-flight forum. Rows are never updated in place; the governance
-- route creates the next version.

-- ── meeting_type — the forum policy ───────────────────────────────────────────────────────
CREATE TABLE meeting_type (
    id                    uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id             uuid NOT NULL DEFAULT app_current_tenant(),
    key                   text NOT NULL,
    name                  text NOT NULL,
    purpose               text NOT NULL DEFAULT '',
    -- The L0 query's sources: open approvals, register items of named kinds, dependency slips,
    -- cadence slots. Shape is validated by the action, not the database (it is policy JSON).
    agenda_sources        jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- Required participation by department/position with minimum counts. The item's own
    -- participating_departments narrows it.
    required_participation jsonb NOT NULL DEFAULT '[]'::jsonb,
    quorum                int NOT NULL DEFAULT 1 CHECK (quorum >= 1),
    decision_rule         text NOT NULL DEFAULT 'majority'
                          CHECK (decision_rule IN ('unanimous', 'majority', 'chair')),
    evidence_minimum      int NOT NULL DEFAULT 0 CHECK (evidence_minimum >= 0),
    -- Durations are intervals, not integers: a decision window and a timebox are policy, and the
    -- store reads them in seconds for the API.
    decision_window       interval NOT NULL DEFAULT interval '7 days'
                          CHECK (decision_window > interval '0'),
    timebox               interval NOT NULL DEFAULT interval '1 hour'
                          CHECK (timebox > interval '0'),
    allowed_outcomes      text[] NOT NULL
                          DEFAULT ARRAY['decision', 'commitment', 'action', 'information', 'deferred']
                          CHECK (allowed_outcomes <@ ARRAY['decision', 'commitment', 'action',
                                                           'information', 'deferred']::text[]),
    -- Who may convene: {kinds[], ids[], roles[]}. Empty lists mean "this dimension is open" so a
    -- tenant can start with a permissive type and tighten it by version, never by a code edit.
    convener_authority    jsonb NOT NULL DEFAULT '{}'::jsonb,
    status                text NOT NULL DEFAULT 'active'
                          CHECK (status IN ('draft', 'active', 'retired')),
    version               bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT meeting_type_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT meeting_type_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT meeting_type_key_version_unique UNIQUE (tenant_id, key, version)
);
CREATE INDEX meeting_type_key_idx ON meeting_type (tenant_id, key, version DESC);

ALTER TABLE meeting_type ENABLE ROW LEVEL SECURITY;
ALTER TABLE meeting_type FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON meeting_type FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── meeting — one instance, one card ─────────────────────────────────────────────────────
CREATE TABLE meeting (
    id                          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id                   uuid NOT NULL DEFAULT app_current_tenant(),
    card_id                     uuid NOT NULL,
    convener_kind               text NOT NULL
                                CHECK (convener_kind IN ('governance_body', 'job_binding', 'app_user')),
    convener_id                 uuid NOT NULL,
    -- The typed column is the FK; convener_id stays the polymorphic reference the plan names.
    convener_governance_body_id uuid,
    meeting_type_id             uuid NOT NULL,
    meeting_type_version        bigint NOT NULL CHECK (meeting_type_version >= 1),
    cadence_slot                uuid,
    opens_at                    timestamptz,
    decision_window_end         timestamptz,
    closed_at                   timestamptz,
    status                      text NOT NULL DEFAULT 'scheduled'
                                CHECK (status IN ('scheduled', 'live', 'closed', 'cancelled')),
    version                     bigint NOT NULL DEFAULT 1,
    created_at                  timestamptz NOT NULL DEFAULT now(),
    updated_at                  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT meeting_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT meeting_card_fk FOREIGN KEY (card_id) REFERENCES card (id),
    CONSTRAINT meeting_type_fk FOREIGN KEY (meeting_type_id) REFERENCES meeting_type (id),
    CONSTRAINT meeting_cadence_fk FOREIGN KEY (cadence_slot) REFERENCES programme_cadence (id),
    CONSTRAINT meeting_convener_body_fk FOREIGN KEY (convener_governance_body_id)
        REFERENCES governance_body (id),
    CONSTRAINT meeting_body_iff_governance
        CHECK ((convener_kind = 'governance_body') = (convener_governance_body_id IS NOT NULL)),
    CONSTRAINT meeting_body_matches_convener
        CHECK (convener_governance_body_id IS NULL OR convener_id = convener_governance_body_id)
);
-- One live instance per cadence slot (uq_run_live's analogue). A non-cadence meeting has a NULL
-- slot and is deliberately unconstrained: two standing rooms may be live at once.
CREATE UNIQUE INDEX uq_meeting_live
    ON meeting (tenant_id, meeting_type_id, cadence_slot)
    WHERE status = 'live' AND cadence_slot IS NOT NULL;
CREATE INDEX meeting_card_idx ON meeting (tenant_id, card_id);
CREATE INDEX meeting_type_status_idx ON meeting (tenant_id, meeting_type_id, status);

ALTER TABLE meeting ENABLE ROW LEVEL SECURITY;
ALTER TABLE meeting FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON meeting FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── meeting_agenda_item — the resolved objects ───────────────────────────────────────────
CREATE TABLE meeting_agenda_item (
    id                       uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id                uuid NOT NULL DEFAULT app_current_tenant(),
    meeting_id               uuid NOT NULL,
    sequence                 int NOT NULL CHECK (sequence >= 1),
    resolved_kind            text NOT NULL
                             CHECK (resolved_kind IN ('approval', 'register_item', 'asset', 'cadence')),
    resolved_id              uuid NOT NULL,
    participating_departments uuid[] NOT NULL DEFAULT '{}',
    evidence_refs            jsonb NOT NULL DEFAULT '[]'::jsonb,
    outcome                  text CHECK (outcome IN ('decision', 'commitment', 'action',
                                                     'information', 'deferred')),
    defer_reason             text,
    closed_at                timestamptz,
    version                  bigint NOT NULL DEFAULT 1,
    created_at               timestamptz NOT NULL DEFAULT now(),
    updated_at               timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT meeting_agenda_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT meeting_agenda_meeting_fk FOREIGN KEY (meeting_id) REFERENCES meeting (id) ON DELETE CASCADE,
    CONSTRAINT meeting_agenda_sequence_unique UNIQUE (meeting_id, sequence),
    CONSTRAINT meeting_agenda_object_once UNIQUE (meeting_id, resolved_kind, resolved_id),
    -- A decision outcome requires an approval; an approval item may instead be deferred
    -- (inquorate) or still open, so the implication is one-way.
    CONSTRAINT meeting_agenda_decision_requires_approval
        CHECK (outcome IS DISTINCT FROM 'decision' OR resolved_kind = 'approval')
);
CREATE INDEX meeting_agenda_meeting_idx ON meeting_agenda_item (tenant_id, meeting_id, sequence);

ALTER TABLE meeting_agenda_item ENABLE ROW LEVEL SECURITY;
ALTER TABLE meeting_agenda_item FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON meeting_agenda_item FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── meeting_attendance — the seats ───────────────────────────────────────────────────────
CREATE TABLE meeting_attendance (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    meeting_id      uuid NOT NULL,
    participant_kind text NOT NULL CHECK (participant_kind IN ('job_binding', 'app_user')),
    participant_id  uuid NOT NULL,
    seat            text NOT NULL
                    CHECK (seat IN ('chair', 'member', 'presenter', 'observer', 'delegate')),
    department_id   uuid,
    invitation_state text NOT NULL DEFAULT 'invited'
                     CHECK (invitation_state IN ('invited', 'accepted', 'declined', 'tentative')),
    response_state  text NOT NULL DEFAULT 'none'
                    CHECK (response_state IN ('none', 'accepted', 'declined', 'tentative')),
    delegated_from  uuid,
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT meeting_attendance_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT meeting_attendance_meeting_fk FOREIGN KEY (meeting_id) REFERENCES meeting (id) ON DELETE CASCADE,
    CONSTRAINT meeting_attendance_department_fk FOREIGN KEY (department_id) REFERENCES department (id),
    CONSTRAINT meeting_attendance_delegated_fk FOREIGN KEY (delegated_from) REFERENCES meeting_attendance (id),
    CONSTRAINT meeting_attendance_once UNIQUE (meeting_id, participant_kind, participant_id)
);
CREATE INDEX meeting_attendance_meeting_idx ON meeting_attendance (tenant_id, meeting_id, seat);

ALTER TABLE meeting_attendance ENABLE ROW LEVEL SECURITY;
ALTER TABLE meeting_attendance FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON meeting_attendance FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── meeting_position — the deliberation ─────────────────────────────────────────────────
CREATE TABLE meeting_position (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    meeting_id      uuid NOT NULL,
    agenda_item_id  uuid NOT NULL,
    participant_kind text NOT NULL CHECK (participant_kind IN ('job_binding', 'app_user')),
    participant_id  uuid NOT NULL,
    stance          text NOT NULL
                    CHECK (stance IN ('approve', 'approve_with_conditions', 'reject', 'abstain',
                                      'request_evidence')),
    rationale       text NOT NULL DEFAULT '',
    conditions      jsonb NOT NULL DEFAULT '[]'::jsonb,
    commitment      jsonb,
    surface         text NOT NULL CHECK (surface IN ('web', 'telegram', 'mcp')),
    superseded_at   timestamptz,
    recorded_at     timestamptz NOT NULL DEFAULT now(),
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT meeting_position_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT meeting_position_meeting_fk FOREIGN KEY (meeting_id) REFERENCES meeting (id) ON DELETE CASCADE,
    CONSTRAINT meeting_position_item_fk FOREIGN KEY (agenda_item_id) REFERENCES meeting_agenda_item (id) ON DELETE CASCADE
);
-- One live position per participant per item. A changed stance supersedes the old row: the
-- deliberation keeps its history and only the live row is counted.
CREATE UNIQUE INDEX uq_meeting_position_live
    ON meeting_position (agenda_item_id, participant_kind, participant_id)
    WHERE superseded_at IS NULL;
CREATE INDEX meeting_position_meeting_idx ON meeting_position (tenant_id, meeting_id, recorded_at);

ALTER TABLE meeting_position ENABLE ROW LEVEL SECURITY;
ALTER TABLE meeting_position FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON meeting_position FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── approval_decision — the participant extension ───────────────────────────────────────
-- A seat is a human or a job binding. The decision row stays single (dual control is still the
-- (approval_id, participant) uniqueness), and `via` records the meeting surface.
ALTER TABLE approval_decision ALTER COLUMN approver_id DROP NOT NULL;
ALTER TABLE approval_decision ADD COLUMN job_binding_id uuid
    REFERENCES job_binding (id);
ALTER TABLE approval_decision ADD COLUMN participant_kind text NOT NULL DEFAULT 'human'
    CHECK (participant_kind IN ('human', 'job'));
ALTER TABLE approval_decision ADD CONSTRAINT approval_decision_participant_exactly_one
    CHECK (
        (participant_kind = 'human' AND approver_id IS NOT NULL AND job_binding_id IS NULL)
        OR (participant_kind = 'job' AND approver_id IS NULL AND job_binding_id IS NOT NULL)
    );
CREATE UNIQUE INDEX approval_decision_approval_job_key
    ON approval_decision (approval_id, job_binding_id)
    WHERE job_binding_id IS NOT NULL;

-- ── governed_asset — minutes become a kind ──────────────────────────────────────────────
ALTER TABLE governed_asset DROP CONSTRAINT governed_asset_kind_check;
ALTER TABLE governed_asset ADD CONSTRAINT governed_asset_kind_check CHECK (
    kind IN ('requirement', 'principle', 'adr', 'charter', 'closure', 'postmortem',
             'sop_source', 'mandate', 'programme_brief', 'blueprint', 'business_case',
             'refined_vision', 'benefits_strategy', 'benefits_realization_plan',
             'projects_dossier', 'minutes')
);
