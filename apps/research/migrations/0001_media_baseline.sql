--- 0001_media_baseline.sql
--- The media-discovery schema, captured 2026-11-15 as `pg_dump --schema-only`
--- from the reference service's own database: container d86534cd81ce
--- (postgres:18, database `postgres`, alembic head c7a1f4e9b820).
---
--- Squashed from the reference's three alembic revisions, final state only:
---   efdf83adc2e4  initial schema
---   b4d2e8c17f39  tenant_id varchar(255) -> uuid on the four tenant tables,
---                 plus tenant_id_purge_audit (and its sequence), which survives
---   c7a1f4e9b820  download_jobs.license_id varchar(255) NULL (no default/index)
--- `alembic_version` is excluded: this workspace tracks migrations with sqlx.
---
--- SECURITY MODEL — the reference has NO row-level security. Verified against
--- the live reference: pg_policies is empty and relrowsecurity is false on every
--- table. Tenant scoping is application-level only: the four tenant-scoped tables
--- carry `tenant_id uuid NOT NULL` with a btree index (and ix_media_items_url is
--- UNIQUE on (tenant_id, url)), and nothing in the database enforces it. Those
--- columns and constraints are reproduced here and nothing more; no RLS policies
--- are invented.
---
--- The reference also has no triggers (no non-internal rows in pg_trigger), so
--- updated_at is advanced by the application, not by the database. There are no
--- views, materialised views, functions, comments or extensions to reproduce.
---
--- The pg_dump psql wrapper (`\restrict`/`\unrestrict`) and its session-only
--- `SET search_path`/`SET row_security` lines are dropped so the file runs
--- unchanged through sqlx; nothing here depends on them (all names are qualified).
--
-- PostgreSQL database dump
--


-- Dumped from database version 18.7 (Debian 18.7-1.pgdg13+2)
-- Dumped by pg_dump version 18.7 (Debian 18.7-1.pgdg13+2)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: collection_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.collection_items (
    collection_id character varying(36) NOT NULL,
    media_item_id character varying(36) NOT NULL,
    "position" integer NOT NULL,
    added_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: download_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.download_jobs (
    id character varying(36) NOT NULL,
    tenant_id uuid NOT NULL,
    media_item_id character varying(36) NOT NULL,
    status character varying(32) NOT NULL,
    download_type character varying(32) NOT NULL,
    storage_path text,
    error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone,
    license_id character varying(255)
);


--
-- Name: media_collections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.media_collections (
    id character varying(36) NOT NULL,
    tenant_id uuid NOT NULL,
    name character varying(500) NOT NULL,
    description text,
    collection_type character varying(32) NOT NULL,
    source_url text,
    item_count integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: media_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.media_items (
    id character varying(36) NOT NULL,
    tenant_id uuid NOT NULL,
    title text NOT NULL,
    description text,
    url text NOT NULL,
    media_type character varying(32) NOT NULL,
    source character varying(64) NOT NULL,
    external_ids jsonb NOT NULL,
    duration_seconds integer,
    thumbnail_url text,
    author_name character varying(500),
    author_channel_url text,
    published_at timestamp with time zone,
    language character varying(10),
    tags jsonb NOT NULL,
    view_count integer,
    like_count integer,
    quality_score double precision,
    quality_components jsonb,
    metadata jsonb NOT NULL,
    embedding bytea,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: search_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.search_sessions (
    id character varying(36) NOT NULL,
    tenant_id uuid NOT NULL,
    name character varying(500) NOT NULL,
    query text NOT NULL,
    status character varying(32) NOT NULL,
    item_count integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: session_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.session_items (
    session_id character varying(36) NOT NULL,
    media_item_id character varying(36) NOT NULL,
    discovery_method character varying(64) NOT NULL,
    source_adapter character varying(64) NOT NULL,
    quality_score double precision
);


--
-- Name: tenant_id_purge_audit; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tenant_id_purge_audit (
    id integer NOT NULL,
    table_name character varying(63) NOT NULL,
    legacy_value character varying(255) NOT NULL,
    row_count integer NOT NULL,
    purged_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: tenant_id_purge_audit_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.tenant_id_purge_audit_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tenant_id_purge_audit_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tenant_id_purge_audit_id_seq OWNED BY public.tenant_id_purge_audit.id;


--
-- Name: tenant_id_purge_audit id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_id_purge_audit ALTER COLUMN id SET DEFAULT nextval('public.tenant_id_purge_audit_id_seq'::regclass);


--
-- Name: download_jobs download_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.download_jobs
    ADD CONSTRAINT download_jobs_pkey PRIMARY KEY (id);


--
-- Name: media_collections media_collections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.media_collections
    ADD CONSTRAINT media_collections_pkey PRIMARY KEY (id);


--
-- Name: media_items media_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.media_items
    ADD CONSTRAINT media_items_pkey PRIMARY KEY (id);


--
-- Name: collection_items pk_collection_items; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collection_items
    ADD CONSTRAINT pk_collection_items PRIMARY KEY (collection_id, media_item_id);


--
-- Name: session_items pk_session_items; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_items
    ADD CONSTRAINT pk_session_items PRIMARY KEY (session_id, media_item_id);


--
-- Name: search_sessions search_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.search_sessions
    ADD CONSTRAINT search_sessions_pkey PRIMARY KEY (id);


--
-- Name: tenant_id_purge_audit tenant_id_purge_audit_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_id_purge_audit
    ADD CONSTRAINT tenant_id_purge_audit_pkey PRIMARY KEY (id);


--
-- Name: tenant_id_purge_audit uq_tenant_purge_audit; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tenant_id_purge_audit
    ADD CONSTRAINT uq_tenant_purge_audit UNIQUE (table_name, legacy_value);


--
-- Name: ix_download_jobs_media_item; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_download_jobs_media_item ON public.download_jobs USING btree (media_item_id);


--
-- Name: ix_download_jobs_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_download_jobs_tenant_id ON public.download_jobs USING btree (tenant_id);


--
-- Name: ix_media_collections_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_media_collections_tenant_id ON public.media_collections USING btree (tenant_id);


--
-- Name: ix_media_items_external_ids; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_media_items_external_ids ON public.media_items USING gin (external_ids jsonb_path_ops);


--
-- Name: ix_media_items_media_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_media_items_media_type ON public.media_items USING btree (tenant_id, media_type);


--
-- Name: ix_media_items_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_media_items_tenant_id ON public.media_items USING btree (tenant_id);


--
-- Name: ix_media_items_url; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX ix_media_items_url ON public.media_items USING btree (tenant_id, url);


--
-- Name: ix_search_sessions_tenant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_search_sessions_tenant_id ON public.search_sessions USING btree (tenant_id);


--
-- Name: collection_items collection_items_collection_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collection_items
    ADD CONSTRAINT collection_items_collection_id_fkey FOREIGN KEY (collection_id) REFERENCES public.media_collections(id) ON DELETE CASCADE;


--
-- Name: collection_items collection_items_media_item_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collection_items
    ADD CONSTRAINT collection_items_media_item_id_fkey FOREIGN KEY (media_item_id) REFERENCES public.media_items(id) ON DELETE CASCADE;


--
-- Name: download_jobs download_jobs_media_item_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.download_jobs
    ADD CONSTRAINT download_jobs_media_item_id_fkey FOREIGN KEY (media_item_id) REFERENCES public.media_items(id) ON DELETE CASCADE;


--
-- Name: session_items session_items_media_item_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_items
    ADD CONSTRAINT session_items_media_item_id_fkey FOREIGN KEY (media_item_id) REFERENCES public.media_items(id) ON DELETE CASCADE;


--
-- Name: session_items session_items_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.session_items
    ADD CONSTRAINT session_items_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.search_sessions(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--


