//! Shared reactive state: session, manifest, active tenant and the Advanced switch.
//!
//! Advanced is one global signal (persisted in `localStorage`), not a per-screen settings
//! panel: it deepens every screen at once with ids, versions, action names and raw JSON.

use leptos::prelude::*;
use serde_json::Value;

use crate::api::{ApiError, Client, Invocation, InvokeOptions, Manifest};
use crate::auth::{self, Config};
use crate::models::Me;

pub const ADVANCED_KEY: &str = "conveyor.advanced";
pub const TENANT_KEY: &str = "conveyor.active_tenant";

#[derive(Clone, Copy)]
pub struct AppCtx {
    pub token: RwSignal<Option<String>>,
    pub tenant: RwSignal<Option<String>>,
    pub me: RwSignal<Option<Me>>,
    pub manifest: RwSignal<Option<Manifest>>,
    pub config: RwSignal<Option<Config>>,
    pub advanced: RwSignal<bool>,
    pub palette_open: RwSignal<bool>,
    pub toast: RwSignal<Option<String>>,
}

impl AppCtx {
    pub fn new() -> Self {
        Self {
            token: RwSignal::new(None),
            tenant: RwSignal::new(local_get(TENANT_KEY)),
            me: RwSignal::new(None),
            manifest: RwSignal::new(None),
            config: RwSignal::new(None),
            advanced: RwSignal::new(local_get(ADVANCED_KEY).as_deref() == Some("1")),
            palette_open: RwSignal::new(false),
            toast: RwSignal::new(None),
        }
    }

    pub fn client(&self) -> Client {
        Client {
            token: self.token.get_untracked(),
            tenant: self.tenant.get_untracked(),
        }
    }

    /// The tenant currently acted in, from the server's `/api/me` answer.
    ///
    /// `active_tenant` is `None` until the caller names one; the locally remembered choice
    /// matches a membership first, then the server's answer (when it selected one).
    pub fn active_tenant(&self) -> Option<crate::models::MembershipView> {
        let me = self.me.get()?;
        let selected = self.tenant.get_untracked();
        me.memberships
            .iter()
            .find(|membership| Some(&membership.tenant_id) == selected.as_ref())
            .cloned()
            .or_else(|| me.active_tenant.clone())
            .or_else(|| me.memberships.first().cloned())
    }

    pub fn role(&self) -> Option<String> {
        self.active_tenant().map(|tenant| tenant.role)
    }

    pub fn is_super_admin(&self) -> bool {
        self.me.get().map(|me| me.is_super_admin).unwrap_or(false)
    }

    pub fn has_action(&self, name: &str) -> bool {
        self.manifest
            .get()
            .map(|manifest| manifest.actions.iter().any(|action| action.name == name))
            .unwrap_or(false)
    }

    pub fn action_names(&self) -> Vec<String> {
        self.manifest
            .get()
            .map(|manifest| {
                manifest
                    .actions
                    .into_iter()
                    .map(|action| action.name)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Switch tenant, persist the choice, and re-read the two role-filtered documents.
    pub fn switch_tenant(&self, tenant_id: String) {
        self.tenant.set(Some(tenant_id.clone()));
        local_set(TENANT_KEY, &tenant_id);
        let ctx = *self;
        leptos::task::spawn_local(async move {
            let client = ctx.client();
            if let Ok(me) = auth::load_me(&client).await {
                ctx.me.set(Some(me));
            }
            if let Ok(manifest) = client.manifest().await {
                ctx.manifest.set(Some(manifest));
            }
        });
    }

    pub async fn invoke(
        &self,
        name: &str,
        input: Value,
        options: InvokeOptions,
    ) -> Result<Invocation, ApiError> {
        self.client().invoke(name, input, options).await
    }

    pub fn report(&self, error: &ApiError) {
        self.toast.set(Some(error.to_string()));
    }
}

impl Default for AppCtx {
    fn default() -> Self {
        Self::new()
    }
}

pub fn local_get(key: &str) -> Option<String> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(key).ok().flatten())
}

pub fn local_set(key: &str, value: &str) {
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.set_item(key, value);
    }
}

pub fn local_remove(key: &str) {
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item(key);
    }
}
