--
-- PostgreSQL database dump
--


-- Dumped from database version 18.6 (Ubuntu 18.6-1.pgdg24.04+2)
-- Dumped by pg_dump version 18.6 (Ubuntu 18.6-1.pgdg24.04+2)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;

--
-- Name: public; Type: SCHEMA; Schema: -; Owner: -
--

-- *not* creating schema, since initdb creates it


--
-- Name: pg_stat_statements; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_stat_statements WITH SCHEMA public;


--
-- Name: EXTENSION pg_stat_statements; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pg_stat_statements IS 'track planning and execution statistics of all SQL statements executed';


--
-- Name: pg_trgm; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;


--
-- Name: EXTENSION pg_trgm; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pg_trgm IS 'text similarity measurement and index searching based on trigrams';


--
-- Name: pgcrypto; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;


--
-- Name: EXTENSION pgcrypto; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pgcrypto IS 'cryptographic functions';


--
-- Name: uuid-ossp; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS "uuid-ossp" WITH SCHEMA public;


--
-- Name: EXTENSION "uuid-ossp"; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION "uuid-ossp" IS 'generate universally unique identifiers (UUIDs)';


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: api_client; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.api_client (
    realm character varying(255) NOT NULL,
    client_id character varying(255) NOT NULL,
    owner_key character varying(64) NOT NULL,
    cross_owner boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: app; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.app (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    key text NOT NULL,
    name text NOT NULL,
    description text,
    revoked boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    audience character varying(16) DEFAULT 'platform'::character varying NOT NULL,
    CONSTRAINT ck_app_audience CHECK (((audience)::text = ANY ((ARRAY['human'::character varying, 'platform'::character varying, 'automation'::character varying])::text[])))
);


--
-- Name: app_issuer; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.app_issuer (
    app_id uuid NOT NULL,
    owner_key character varying(64) NOT NULL
);


--
-- Name: app_requires; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.app_requires (
    app_id uuid NOT NULL,
    required_app_id uuid NOT NULL,
    caller_owner_key character varying(64) NOT NULL,
    CONSTRAINT ck_app_requires_not_self CHECK ((app_id <> required_app_id))
);


--
-- Name: bundle; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bundle (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    key character varying(64) NOT NULL,
    name character varying(255) NOT NULL,
    description text,
    revoked boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: bundle_app; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bundle_app (
    bundle_id uuid NOT NULL,
    app_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: bundle_caller; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bundle_caller (
    bundle_id uuid NOT NULL,
    owner_key character varying(64) NOT NULL,
    app_id uuid NOT NULL
);


--
-- Name: bundle_ensurer; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bundle_ensurer (
    bundle_id uuid NOT NULL,
    owner_key character varying(64) NOT NULL
);


--
-- Name: license; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.license (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    subject_id uuid NOT NULL,
    app_id uuid NOT NULL,
    plan_id uuid NOT NULL,
    region_id uuid,
    status text NOT NULL,
    starts_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_at timestamp with time zone,
    metadata jsonb,
    tenant_id uuid NOT NULL,
    metering_provisioned_at timestamp with time zone,
    metering_verified_at timestamp with time zone,
    deleted_at timestamp with time zone,
    deleted_by text,
    delete_reason text,
    deleted_from_status character varying(50),
    bundle_id uuid,
    token_budget_override integer,
    quota_override integer,
    is_dependency boolean DEFAULT false NOT NULL,
    CONSTRAINT ck_license_dependency_unbundled CHECK (((NOT is_dependency) OR (bundle_id IS NULL))),
    CONSTRAINT ck_license_quota_override_nonneg CHECK (((quota_override IS NULL) OR (quota_override >= 0))),
    CONSTRAINT ck_license_status CHECK ((status = ANY (ARRAY['active'::text, 'revoked'::text, 'expired'::text, 'pending'::text]))),
    CONSTRAINT ck_license_token_budget_override_nonneg CHECK (((token_budget_override IS NULL) OR (token_budget_override >= 0)))
);


--
-- Name: COLUMN license.deleted_at; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.license.deleted_at IS 'When this licence was SOFT-deleted. NULL = not deleted. A row with a value here is retained solely so a historical seat count stays derivable: it is excluded from every present-tense answer, and included by models.effectively_active_clauses only for an as_of EARLIER than this timestamp. Written only when LICENSE_SOFT_DELETE is armed.';


--
-- Name: COLUMN license.deleted_by; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.license.deleted_by IS 'The `sub` of the admin or service account that deleted this licence. TEXT, not an FK to subject: the actor is frequently a service account with no subject row in this registry.';


--
-- Name: COLUMN license.delete_reason; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.license.delete_reason IS 'Caller-supplied reason for the deletion. Free text — the only place the WHY of a destructive request is recorded.';


--
-- Name: COLUMN license.deleted_from_status; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.license.deleted_from_status IS 'The value `status` held at the instant of the soft delete. NOT redundant: the soft delete must set status=''revoked'' to free the partial unique index uq_license_active_tenant_app_subject (WHERE status = ''active''), so without this column a past-instant seat count could not tell a licence that was ACTIVE when deleted from one that had already been revoked, and would over-count the second.';


--
-- Name: COLUMN license.bundle_id; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.license.bundle_id IS 'The bundle this ONE licence speaks for (set by scripts/provision_tenant_bundle.py). NULL = a single-app licence. When set, /license/introspect answers apps[] = the keys of the bundle''s apps, and a service accepts the call when its app is in apps[].';


--
-- Name: COLUMN license.token_budget_override; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.license.token_budget_override IS 'Per-licence monthly token budget pushed to billing instead of plan.token_budget_monthly. NULL = plan value. Read by provisioning AND the metering reconcile sweep (#239).';


--
-- Name: COLUMN license.quota_override; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.license.quota_override IS 'Per-licence monthly request quota pushed to billing instead of plan.quota_monthly. NULL = plan value. Read by provisioning AND the metering reconcile sweep (#239).';


--
-- Name: offering; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.offering (
    id uuid NOT NULL,
    app_id uuid NOT NULL,
    code character varying(64) NOT NULL,
    name character varying(255) NOT NULL,
    byo_mode character varying(32) NOT NULL,
    markup_pct numeric(6,2),
    surcharge_input_per_1m_usd numeric(20,8),
    surcharge_output_per_1m_usd numeric(20,8),
    surcharge_cache_creation_per_1m_usd numeric(20,8),
    surcharge_cache_read_per_1m_usd numeric(20,8),
    surcharge_per_request_usd numeric(20,8),
    notes text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    access_price_usd numeric(20,8),
    access_cadence character varying(16) DEFAULT 'monthly'::character varying NOT NULL,
    included_allowance_usd numeric(20,8),
    CONSTRAINT ck_offering_access_cadence CHECK (((access_cadence)::text = ANY ((ARRAY['monthly'::character varying, 'one_time'::character varying])::text[]))),
    CONSTRAINT ck_offering_byo_mode CHECK (((byo_mode)::text = ANY ((ARRAY['local_allowed'::character varying, 'byo_required'::character varying, 'platform_only'::character varying])::text[])))
);


--
-- Name: plan; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.plan (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    app_id uuid NOT NULL,
    code text NOT NULL,
    name text NOT NULL,
    description text,
    region_id uuid,
    duration_days integer,
    revoked boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    rate_limit_rpm integer DEFAULT 60 NOT NULL,
    rate_limit_burst integer DEFAULT 10 NOT NULL,
    quota_monthly integer,
    token_budget_monthly integer,
    CONSTRAINT ck_plan_quota_monthly_nonneg CHECK (((quota_monthly IS NULL) OR (quota_monthly >= 0)))
);


--
-- Name: realm_owner; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.realm_owner (
    realm character varying(255) NOT NULL,
    owner_key character varying(64) NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: region; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.region (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    code text NOT NULL,
    name text NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: schema_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.schema_migrations (
    filename text NOT NULL,
    applied_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: subject; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.subject (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    keycloak_sub text NOT NULL,
    keycloak_realm text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    tenant_id uuid,
    actor_sub uuid
);


--
-- Name: tenant_app_byo_mode; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tenant_app_byo_mode (
    tenant_id uuid NOT NULL,
    app_id uuid NOT NULL,
    byo_mode character varying(32) NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    offering_code character varying(64),
    CONSTRAINT ck_tenant_app_byo_mode CHECK (((byo_mode)::text = ANY ((ARRAY['local_allowed'::character varying, 'byo_required'::character varying, 'platform_only'::character varying])::text[])))
);


--
-- Name: tenant_app_grant; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tenant_app_grant (
    id uuid DEFAULT uuidv7() NOT NULL,
    tenant_id uuid NOT NULL,
    grantee_owner_key character varying(64) NOT NULL,
    granted_by text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_at timestamp with time zone
);


--
-- Name: tenant_bundle; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tenant_bundle (
    tenant_id uuid NOT NULL,
    bundle_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: tenant_dependency_grant; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tenant_dependency_grant (
    tenant_id uuid NOT NULL,
    owner_key character varying(64) NOT NULL,
    app_id uuid NOT NULL,
    granted_by text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: tenant_owner; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tenant_owner (
    key character varying(64) NOT NULL,
    display_name character varying(255) NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT ck_tenant_owner_key_format CHECK (((key)::text ~ '^[a-z0-9][a-z0-9-]*$'::text))
);


--
-- Name: tenants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tenants (
    id uuid NOT NULL,
    name character varying(255) NOT NULL,
    keycloak_realm character varying(255),
    created_at timestamp with time zone NOT NULL,
    byo_mode character varying(32) DEFAULT 'byo_required'::character varying NOT NULL,
    archived_at timestamp with time zone,
    kind character varying(16) DEFAULT 'unclassified'::character varying NOT NULL,
    cost_centre character varying(64),
    budget_currency character(3),
    owner_key character varying(64) NOT NULL,
    external_ref character varying(255),
    CONSTRAINT ck_tenants_budget_currency CHECK ((budget_currency = ANY (ARRAY['EUR'::bpchar, 'USD'::bpchar, 'CAD'::bpchar]))),
    CONSTRAINT ck_tenants_byo_mode CHECK (((byo_mode)::text = ANY ((ARRAY['local_allowed'::character varying, 'byo_required'::character varying, 'platform_only'::character varying])::text[]))),
    CONSTRAINT ck_tenants_cost_centre CHECK (((cost_centre IS NULL) OR ((cost_centre)::text = ANY ((ARRAY['service'::character varying, 'tests'::character varying, 'customers'::character varying, 'ci'::character varying, 'improvement'::character varying, 'merqi'::character varying])::text[])))),
    CONSTRAINT ck_tenants_kind CHECK (((kind)::text = ANY ((ARRAY['customer'::character varying, 'internal'::character varying, 'service'::character varying, 'test'::character varying, 'unclassified'::character varying])::text[]))),
    CONSTRAINT ck_tenants_name_format CHECK (((name)::text ~ '^[a-z0-9][a-z0-9-]*$'::text)),
    CONSTRAINT ck_tenants_service_not_byo_required CHECK ((NOT (((kind)::text = 'service'::text) AND ((byo_mode)::text = 'byo_required'::text))))
);


--
-- Name: token; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.token (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    license_id uuid NOT NULL,
    token_hash text NOT NULL,
    token_type text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone,
    revoked boolean DEFAULT false NOT NULL,
    revoked_at timestamp with time zone,
    last_used_at timestamp with time zone,
    granted_roles text[] DEFAULT '{}'::text[] NOT NULL,
    long_lived_owner text,
    long_lived_reason text,
    CONSTRAINT ck_token_granted_roles_allowlist CHECK ((granted_roles <@ ARRAY['platform-admin'::text])),
    CONSTRAINT ck_token_granted_roles_short_lived CHECK (((cardinality(granted_roles) = 0) OR ((expires_at IS NOT NULL) AND (expires_at <= (created_at + '01:05:00'::interval))))),
    CONSTRAINT ck_token_long_lived_registered CHECK ((((long_lived_owner IS NULL) AND (long_lived_reason IS NULL)) OR ((expires_at IS NULL) AND (cardinality(granted_roles) = 0) AND (length(btrim(long_lived_owner)) > 0) AND (length(btrim(long_lived_reason)) > 0)))),
    CONSTRAINT ck_token_token_type CHECK ((token_type = ANY (ARRAY['api'::text, 'node'::text, 'internal'::text])))
);


--
-- Name: COLUMN token.granted_roles; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.token.granted_roles IS 'Roles this ONE token carries on /license/introspect. Empty for every ordinary token. Non-empty only for a short-lived operator token minted by POST /licenses/{id}/operator-token for a caller whose own Keycloak JWT held the role.';


--
-- Name: api_client api_client_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.api_client
    ADD CONSTRAINT api_client_pkey PRIMARY KEY (realm, client_id);


--
-- Name: app_issuer app_issuer_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app_issuer
    ADD CONSTRAINT app_issuer_pkey PRIMARY KEY (app_id, owner_key);


--
-- Name: app app_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app
    ADD CONSTRAINT app_key_key UNIQUE (key);


--
-- Name: app app_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app
    ADD CONSTRAINT app_pkey PRIMARY KEY (id);


--
-- Name: app_requires app_requires_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app_requires
    ADD CONSTRAINT app_requires_pkey PRIMARY KEY (app_id, required_app_id);


--
-- Name: bundle_app bundle_app_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_app
    ADD CONSTRAINT bundle_app_pkey PRIMARY KEY (bundle_id, app_id);


--
-- Name: bundle_caller bundle_caller_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_caller
    ADD CONSTRAINT bundle_caller_pkey PRIMARY KEY (bundle_id, owner_key, app_id);


--
-- Name: bundle_ensurer bundle_ensurer_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_ensurer
    ADD CONSTRAINT bundle_ensurer_pkey PRIMARY KEY (bundle_id, owner_key);


--
-- Name: bundle bundle_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle
    ADD CONSTRAINT bundle_key_key UNIQUE (key);


--
-- Name: bundle bundle_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle
    ADD CONSTRAINT bundle_pkey PRIMARY KEY (id);


--
-- Name: license license_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.license
    ADD CONSTRAINT license_pkey PRIMARY KEY (id);


--
-- Name: offering offering_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.offering
    ADD CONSTRAINT offering_pkey PRIMARY KEY (id);


--
-- Name: plan plan_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.plan
    ADD CONSTRAINT plan_pkey PRIMARY KEY (id);


--
-- Name: realm_owner realm_owner_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.realm_owner
    ADD CONSTRAINT realm_owner_pkey PRIMARY KEY (realm);


--
-- Name: region region_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.region
    ADD CONSTRAINT region_code_key UNIQUE (code);


--
-- Name: region region_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.region
    ADD CONSTRAINT region_pkey PRIMARY KEY (id);


--
-- Name: schema_migrations schema_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.schema_migrations
    ADD CONSTRAINT schema_migrations_pkey PRIMARY KEY (filename);


--
-- Name: subject subject_keycloak_sub_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.subject
    ADD CONSTRAINT subject_keycloak_sub_key UNIQUE (keycloak_sub);


--
-- Name: subject subject_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.subject
    ADD CONSTRAINT subject_pkey PRIMARY KEY (id);


--
-- Name: tenant_app_byo_mode tenant_app_byo_mode_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_app_byo_mode
    ADD CONSTRAINT tenant_app_byo_mode_pkey PRIMARY KEY (tenant_id, app_id);


--
-- Name: tenant_app_grant tenant_app_grant_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_app_grant
    ADD CONSTRAINT tenant_app_grant_pkey PRIMARY KEY (id);


--
-- Name: tenant_bundle tenant_bundle_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_bundle
    ADD CONSTRAINT tenant_bundle_pkey PRIMARY KEY (tenant_id, bundle_id);


--
-- Name: tenant_dependency_grant tenant_dependency_grant_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_dependency_grant
    ADD CONSTRAINT tenant_dependency_grant_pkey PRIMARY KEY (tenant_id, owner_key, app_id);


--
-- Name: tenant_owner tenant_owner_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_owner
    ADD CONSTRAINT tenant_owner_pkey PRIMARY KEY (key);


--
-- Name: tenants tenants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenants
    ADD CONSTRAINT tenants_pkey PRIMARY KEY (id);


--
-- Name: token token_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.token
    ADD CONSTRAINT token_pkey PRIMARY KEY (id);


--
-- Name: offering ux_offering_app_code; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.offering
    ADD CONSTRAINT ux_offering_app_code UNIQUE (app_id, code);


--
-- Name: idx_bundle_app_app; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_bundle_app_app ON public.bundle_app USING btree (app_id);


--
-- Name: idx_license_app; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_license_app ON public.license USING btree (app_id);


--
-- Name: idx_license_bundle; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_license_bundle ON public.license USING btree (bundle_id);


--
-- Name: idx_license_expires_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_license_expires_at ON public.license USING btree (expires_at);


--
-- Name: idx_license_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_license_status ON public.license USING btree (status);


--
-- Name: idx_license_subject; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_license_subject ON public.license USING btree (subject_id);


--
-- Name: idx_license_tenant; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_license_tenant ON public.license USING btree (tenant_id);


--
-- Name: idx_offering_app; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_offering_app ON public.offering USING btree (app_id);


--
-- Name: idx_subject_realm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_subject_realm ON public.subject USING btree (keycloak_realm);


--
-- Name: idx_subject_tenant; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_subject_tenant ON public.subject USING btree (tenant_id);


--
-- Name: idx_tenant_app_byo_mode_tenant; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_tenant_app_byo_mode_tenant ON public.tenant_app_byo_mode USING btree (tenant_id);


--
-- Name: idx_tenant_bundle_bundle; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_tenant_bundle_bundle ON public.tenant_bundle USING btree (bundle_id);


--
-- Name: idx_token_expires_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_token_expires_at ON public.token USING btree (expires_at);


--
-- Name: idx_token_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_token_hash ON public.token USING btree (token_hash);


--
-- Name: idx_token_license; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_token_license ON public.token USING btree (license_id);


--
-- Name: idx_token_revoked; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_token_revoked ON public.token USING btree (revoked);


--
-- Name: ix_tenants_kind; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_tenants_kind ON public.tenants USING btree (kind);


--
-- Name: ix_tenants_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_tenants_name ON public.tenants USING btree (name);


--
-- Name: uq_license_active_tenant_app_subject; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_license_active_tenant_app_subject ON public.license USING btree (tenant_id, app_id, subject_id) WHERE (status = 'active'::text);


--
-- Name: uq_tenant_app_grant_active; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_tenant_app_grant_active ON public.tenant_app_grant USING btree (tenant_id, grantee_owner_key) WHERE (revoked_at IS NULL);


--
-- Name: uq_tenants_owner_external_ref; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_tenants_owner_external_ref ON public.tenants USING btree (owner_key, external_ref) WHERE ((external_ref IS NOT NULL) AND (archived_at IS NULL));


--
-- Name: uq_tenants_owner_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_tenants_owner_name ON public.tenants USING btree (owner_key, name) WHERE (archived_at IS NULL);


--
-- Name: ux_plan_app_code_region; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX ux_plan_app_code_region ON public.plan USING btree (app_id, code, COALESCE(region_id, '00000000-0000-0000-0000-000000000000'::uuid));


--
-- Name: ux_token_license_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX ux_token_license_hash ON public.token USING btree (license_id, token_hash);


--
-- Name: bundle_app bundle_app_app_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_app
    ADD CONSTRAINT bundle_app_app_id_fkey FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: bundle_app bundle_app_bundle_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_app
    ADD CONSTRAINT bundle_app_bundle_id_fkey FOREIGN KEY (bundle_id) REFERENCES public.bundle(id) ON DELETE CASCADE;


--
-- Name: api_client fk_api_client_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.api_client
    ADD CONSTRAINT fk_api_client_owner FOREIGN KEY (owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: app_issuer fk_app_issuer_app; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app_issuer
    ADD CONSTRAINT fk_app_issuer_app FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: app_issuer fk_app_issuer_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app_issuer
    ADD CONSTRAINT fk_app_issuer_owner FOREIGN KEY (owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: app_requires fk_app_requires_app; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app_requires
    ADD CONSTRAINT fk_app_requires_app FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: app_requires fk_app_requires_caller_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app_requires
    ADD CONSTRAINT fk_app_requires_caller_owner FOREIGN KEY (caller_owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: app_requires fk_app_requires_required_app; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app_requires
    ADD CONSTRAINT fk_app_requires_required_app FOREIGN KEY (required_app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: bundle_caller fk_bundle_caller_app; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_caller
    ADD CONSTRAINT fk_bundle_caller_app FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: bundle_caller fk_bundle_caller_bundle; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_caller
    ADD CONSTRAINT fk_bundle_caller_bundle FOREIGN KEY (bundle_id) REFERENCES public.bundle(id) ON DELETE CASCADE;


--
-- Name: bundle_caller fk_bundle_caller_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_caller
    ADD CONSTRAINT fk_bundle_caller_owner FOREIGN KEY (owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: bundle_ensurer fk_bundle_ensurer_bundle; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_ensurer
    ADD CONSTRAINT fk_bundle_ensurer_bundle FOREIGN KEY (bundle_id) REFERENCES public.bundle(id) ON DELETE CASCADE;


--
-- Name: bundle_ensurer fk_bundle_ensurer_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_ensurer
    ADD CONSTRAINT fk_bundle_ensurer_owner FOREIGN KEY (owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: license fk_license_bundle; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.license
    ADD CONSTRAINT fk_license_bundle FOREIGN KEY (bundle_id) REFERENCES public.bundle(id) ON DELETE RESTRICT;


--
-- Name: license fk_license_tenant; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.license
    ADD CONSTRAINT fk_license_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE RESTRICT;


--
-- Name: realm_owner fk_realm_owner_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.realm_owner
    ADD CONSTRAINT fk_realm_owner_owner FOREIGN KEY (owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: subject fk_subject_tenant; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.subject
    ADD CONSTRAINT fk_subject_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE RESTRICT;


--
-- Name: tenant_app_grant fk_tenant_app_grant_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_app_grant
    ADD CONSTRAINT fk_tenant_app_grant_owner FOREIGN KEY (grantee_owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: tenant_app_grant fk_tenant_app_grant_tenant; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_app_grant
    ADD CONSTRAINT fk_tenant_app_grant_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE RESTRICT;


--
-- Name: tenant_bundle fk_tenant_bundle_bundle; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_bundle
    ADD CONSTRAINT fk_tenant_bundle_bundle FOREIGN KEY (bundle_id) REFERENCES public.bundle(id) ON DELETE RESTRICT;


--
-- Name: tenant_bundle fk_tenant_bundle_tenant; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_bundle
    ADD CONSTRAINT fk_tenant_bundle_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE RESTRICT;


--
-- Name: tenant_dependency_grant fk_tenant_dependency_grant_app; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_dependency_grant
    ADD CONSTRAINT fk_tenant_dependency_grant_app FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: tenant_dependency_grant fk_tenant_dependency_grant_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_dependency_grant
    ADD CONSTRAINT fk_tenant_dependency_grant_owner FOREIGN KEY (owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: tenant_dependency_grant fk_tenant_dependency_grant_tenant; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_dependency_grant
    ADD CONSTRAINT fk_tenant_dependency_grant_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE RESTRICT;


--
-- Name: tenants fk_tenants_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenants
    ADD CONSTRAINT fk_tenants_owner FOREIGN KEY (owner_key) REFERENCES public.tenant_owner(key) ON DELETE RESTRICT;


--
-- Name: license license_app_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.license
    ADD CONSTRAINT license_app_id_fkey FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: license license_plan_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.license
    ADD CONSTRAINT license_plan_id_fkey FOREIGN KEY (plan_id) REFERENCES public.plan(id) ON DELETE RESTRICT;


--
-- Name: license license_region_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.license
    ADD CONSTRAINT license_region_id_fkey FOREIGN KEY (region_id) REFERENCES public.region(id);


--
-- Name: license license_subject_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.license
    ADD CONSTRAINT license_subject_id_fkey FOREIGN KEY (subject_id) REFERENCES public.subject(id) ON DELETE CASCADE;


--
-- Name: offering offering_app_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.offering
    ADD CONSTRAINT offering_app_id_fkey FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: plan plan_app_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.plan
    ADD CONSTRAINT plan_app_id_fkey FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: plan plan_region_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.plan
    ADD CONSTRAINT plan_region_id_fkey FOREIGN KEY (region_id) REFERENCES public.region(id);


--
-- Name: tenant_app_byo_mode tenant_app_byo_mode_app_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_app_byo_mode
    ADD CONSTRAINT tenant_app_byo_mode_app_id_fkey FOREIGN KEY (app_id) REFERENCES public.app(id) ON DELETE CASCADE;


--
-- Name: tenant_app_byo_mode tenant_app_byo_mode_tenant_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_app_byo_mode
    ADD CONSTRAINT tenant_app_byo_mode_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE CASCADE;


--
-- Name: token token_license_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.token
    ADD CONSTRAINT token_license_id_fkey FOREIGN KEY (license_id) REFERENCES public.license(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--
