# 50 — Source schema inventory (live, 2026-11-07)

Measured on **2026-11-07** from master Postgres (`sudo -u postgres psql -d <db>`, `\dt`), not from migrations.
This file is the **raw inventory** only: the complete `public` table lists, verbatim. The **total disposition** —
where each table lands (a target entity, or the dropped list with a reason) — lives in
`02-DATA-ARCHITECTURE.md` §7 (Source mapping — TOTAL), and every table listed here appears there. Nothing in this
file is a mapping decision.

Counts measured 2026-11-07 (`pg_tables` / `pg_class` in `fr`, `requirements_api`, `kanban`): **fr 86 domain
tables** (88 base tables in `public`, 90 relations including two `pg_stat_statements*` views), **RM 57 domain
tables** (58 base tables including `alembic_version`), **Planka 33 domain tables** (35 base tables including
`migration` and `migration_lock`). Queries: `select count(*) from pg_tables where schemaname='public'`;
`select relkind, count(*) from pg_class c join pg_namespace n on n.oid=c.relnamespace where n.nspname='public'
and c.relkind in ('r','p','v','m') group by relkind`.

Bookkeeping tables are listed for completeness and are **not** domain surfaces: `_migration_effects`,
`_migrations`, `pg_stat_statements`, `pg_stat_statements_info`, `alembic_version`, `migration`, `migration_lock`.

## fr — database `fr` (86 domain tables)

action_idempotency, agent_members, agent_runtime_sessions, agent_sessions, analytics_rollup_meta, analytics_rollups,
andon_state, andon_state_history, approval_callback_urls, approval_decisions, approvals, asset_assignments,
asset_bindings, asset_stream_membership, budget_alert_receipts, budget_topups, capability_audit, card_asset_binding,
card_bridge_links, card_computed_checks, card_contracts, card_done_overrides, card_drafts, card_executed_activities,
card_phase_lease, card_phase_lease_waiter, card_plan_events, card_plan_pins, card_scenario_evidence, card_seeker_links,
card_version_stamps, channels, coding_analysis_runs, comments, contract_dependencies, conveyor_flow_runs,
decompositions, department_members, departments, documents, engagement_deliveries, execution_signoff_blocks,
family_bundles, family_decisions, finding_suppression_events, finding_suppressions, github_app_installations,
inapp_messages, interface_validations, katas, member_knowledge_areas, n8n_execution_mapping, notification_log,
notification_prefs, notifications, priority_change_events, project_asset_copy_intents, project_conveyor_failures,
project_conveyor_start_attempts, project_deliverables, project_metrics, project_milestones, project_outlines,
project_reports, project_repositories, project_request_repositories, project_requests, project_stakeholders,
projects, provisioning_audit, repositories, required_approvers, router_dedupe, scheduled_trigger_runs,
scheduled_triggers, signup_approvals, streams, suggested_schedules, super_admin_tenant_actions, telegram_forum_topics,
tenant_improvement_projects, user_channel_prefs, user_roles, user_secrets, wip_slot, workspaces

## requirements_management — database `requirements_api` (57 domain tables)

architecture_element_realisation_bindings, architecture_elements, architecture_relationships, architecture_standards,
architecture_view_connections, architecture_view_objects, architecture_views, decisions, entity_version_history,
idempotency_keys, modeling_change_sets, modeling_model_revisions, modeling_models, modeling_publications,
modeling_reviews, modeling_trace_links, modeling_validation_findings, modeling_validation_runs, pm_benefits,
pm_board_templates, pm_capabilities, pm_card_ledger, pm_card_types, pm_coach_actions, pm_deliverable_kinds, pm_gates,
pm_milestone_kinds, pm_priority_levels, pm_programmes, pm_project_brief_versions, pm_project_capabilities,
pm_project_capability_events, pm_project_phase_events, pm_project_phases, pm_project_type_card_types,
pm_project_type_phases, pm_project_types, pm_projects, pm_recipe_items, pm_recipes, pm_resources, pm_risks, pm_wip_limits,
principle_conflicts, principles, quality_evaluations, requirements, tenant_id_crosswalk, toc_nodes, toc_settings,
toc_trees, uml_diagram_connections, uml_diagram_objects, uml_diagrams, uml_elements, uml_interactions, uml_relationships

## Planka — database `kanban` (33 domain tables)

_conveyor_reset_20260922 (a 2026-09-22 reset marker, not a feature), action, attachment, background_image,
base_custom_field_group, board, board_membership, board_subscription, card, card_label, card_membership,
card_subscription, comment, config, custom_field, custom_field_group, custom_field_value, identity_provider_user,
internal_config, label, list, notification, notification_service, project, project_favorite, project_manager, session,
storage_usage, task, task_list, uploaded_file, user_account, webhook

## Live row counts that matter (2026-11-07)

- `kanban.card = 5`, `kanban.board = 4`, `kanban.custom_field_value = 0`, `kanban.label = 0`, `kanban.task = 0` —
  **test data only**; usage cannot be inferred from it.
- `fr.approvals = 0`, `fr.approval_decisions = 0`, `fr.card_drafts = 0`, `fr.card_phase_lease = 0`,
  `fr.conveyor_flow_runs = 693` — the app is idle; only the run history is non-empty.
- `requirements_api.pm_card_types = 293`, `pm_project_type_card_types = 282`, `pm_capabilities = 68`,
  `pm_project_types = 26` — real catalogue data.
- `n8n.workflow_entity = 348` (332 active) with 2,904 Planka nodes, 746 requirements nodes, 1,342 coding nodes.
