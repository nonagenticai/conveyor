#![forbid(unsafe_code)]
//! `#[action]` — one declaration, four artefacts (S04).
//!
//! The attribute is applied to an async handler:
//!
//! ```ignore
//! #[action(name = "members.set_role", version = 1, kind = "internal",
//!          scopes = ["members:write"], idempotent = true, timeout_ms = 30000,
//!          effect = "write", confirm = "none", tenancy = "tenant")]
//! async fn members_set_role(
//!     tx: &mut sqlx::PgConnection,
//!     ctx: conveyor_actions::HandlerCtx,
//!     input: MembersSetRoleIn,
//! ) -> Result<MemberView, conveyor_actions::ActionError> { … }
//! ```
//!
//! From **those same tokens** it emits:
//!
//! * the `ActionSpec` value (the manifest entry, the OpenAPI operation, the MCP tool and
//!   the route path are all read from it),
//! * an `inventory::submit!` so the registry is assembled without a hand-written list,
//! * one typed wrapper the executor calls after schema validation.
//!
//! The name is validated **at expansion**: `<plural>.<verb>`, exactly one dot, both
//! segments lower_snake_case. A bad name is a compile error, not a boot error — a
//! registry entry that cannot be named is not a registry entry.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{ItemFn, LitBool, LitInt, LitStr, Token, parse_macro_input};

/// The parsed attribute body.
struct ActionArgs {
    name: LitStr,
    version: LitInt,
    kind: LitStr,
    scopes: Vec<LitStr>,
    idempotent: LitBool,
    timeout_ms: LitInt,
    effect: LitStr,
    confirm: LitStr,
    tenancy: LitStr,
    /// S10 §5.2: the copilot's tier (`core` / `searchable`); absent means no hint.
    agent_tier: Option<LitStr>,
    /// S10 §5.3: propose this action even when its effect alone would not.
    proposal_required: Option<LitBool>,
    /// S06c: `/mcp` may run this action as a task (`tasks/get|update|cancel`).
    task: Option<LitBool>,
    /// S16: the engine writes an `inference_call` ledger row for this action's successful
    /// external call.
    captures_usage: Option<LitBool>,
}

impl Parse for ActionArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut version = None;
        let mut kind = None;
        let mut scopes = None;
        let mut idempotent = None;
        let mut timeout_ms = None;
        let mut effect = None;
        let mut confirm = None;
        let mut tenancy = None;
        let mut agent_tier = None;
        let mut proposal_required = None;
        let mut task = None;
        let mut captures_usage = None;

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "name" => name = Some(input.parse()?),
                "version" => version = Some(input.parse()?),
                "kind" => kind = Some(input.parse()?),
                "idempotent" => idempotent = Some(input.parse()?),
                "timeout_ms" => timeout_ms = Some(input.parse()?),
                "effect" => effect = Some(input.parse()?),
                "confirm" => confirm = Some(input.parse()?),
                "tenancy" => tenancy = Some(input.parse()?),
                "agent_tier" => agent_tier = Some(input.parse()?),
                "proposal_required" => proposal_required = Some(input.parse()?),
                "task" => task = Some(input.parse()?),
                "captures_usage" => captures_usage = Some(input.parse()?),
                "scopes" => {
                    let content;
                    syn::bracketed!(content in input);
                    let list = Punctuated::<LitStr, Token![,]>::parse_terminated(&content)?;
                    scopes = Some(list.into_iter().collect());
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown #[action] key {other:?}"),
                    ));
                }
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(ActionArgs {
            name: name.ok_or_else(|| input.error("missing name"))?,
            version: version.ok_or_else(|| input.error("missing version"))?,
            kind: kind.ok_or_else(|| input.error("missing kind"))?,
            scopes: scopes.ok_or_else(|| input.error("missing scopes"))?,
            idempotent: idempotent.ok_or_else(|| input.error("missing idempotent"))?,
            timeout_ms: timeout_ms.ok_or_else(|| input.error("missing timeout_ms"))?,
            effect: effect.ok_or_else(|| input.error("missing effect"))?,
            confirm: confirm.ok_or_else(|| input.error("missing confirm"))?,
            tenancy: tenancy.ok_or_else(|| input.error("missing tenancy"))?,
            agent_tier,
            proposal_required,
            task,
            captures_usage,
        })
    }
}

/// Why a name is not a valid action name (CONTRACT §6).
fn name_error(name: &str) -> Option<String> {
    let Some((resource, verb)) = name.split_once('.') else {
        return Some("must be exactly <resource>.<verb>".into());
    };
    if verb.contains('.') {
        return Some("must contain exactly one dot".into());
    }
    let word = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    if !word(resource) {
        return Some(format!("resource {resource:?} is not lower_snake_case"));
    }
    if !word(verb) {
        return Some(format!("verb {verb:?} is not lower_snake_case"));
    }
    None
}

/// The vocabulary constructors, spelled here so the emitted code needs no string parsing
/// at runtime: an unknown effect/confirm/tenancy/kind is a compile error.
fn variant_path(kind: &str, value: &str) -> syn::Result<String> {
    let v = match (kind, value) {
        ("kind", "internal") => "Internal",
        ("kind", "external") => "External",
        ("kind", "workflow") => "Workflow",
        ("effect", "read") => "Read",
        ("effect", "write") => "Write",
        ("effect", "destructive") => "Destructive",
        ("confirm", "none") => "None",
        ("confirm", "card") => "Card",
        ("confirm", "card-typed") => "CardTyped",
        ("tenancy", "self") => "Self_",
        ("tenancy", "tenant") => "Tenant",
        ("tenancy", "project") => "Project",
        ("tenancy", "platform") => "Platform",
        ("scope", "actions:read") => "ActionsRead",
        ("scope", "actions:write") => "ActionsWrite",
        ("scope", "me:read") => "MeRead",
        ("scope", "tenants:read") => "TenantsRead",
        ("scope", "tenants:write") => "TenantsWrite",
        ("scope", "members:read") => "MembersRead",
        ("scope", "members:write") => "MembersWrite",
        ("scope", "departments:read") => "DepartmentsRead",
        ("scope", "departments:write") => "DepartmentsWrite",
        ("scope", "audit:read") => "AuditRead",
        ("scope", "jobs:read") => "JobsRead",
        ("scope", "jobs:write") => "JobsWrite",
        ("scope", "schedules:read") => "SchedulesRead",
        ("scope", "schedules:write") => "SchedulesWrite",
        ("scope", "events:read") => "EventsRead",
        ("scope", "licences:read") => "LicencesRead",
        ("scope", "projects:read") => "ProjectsRead",
        ("scope", "projects:write") => "ProjectsWrite",
        ("scope", "boards:read") => "BoardsRead",
        ("scope", "boards:write") => "BoardsWrite",
        ("scope", "lists:read") => "ListsRead",
        ("scope", "lists:write") => "ListsWrite",
        ("scope", "cards:read") => "CardsRead",
        ("scope", "cards:write") => "CardsWrite",
        ("scope", "runs:read") => "RunsRead",
        ("scope", "runs:write") => "RunsWrite",
        ("scope", "approvals:read") => "ApprovalsRead",
        ("scope", "approvals:write") => "ApprovalsWrite",
        ("scope", "andon:write") => "AndonWrite",
        ("scope", "messages:write") => "MessagesWrite",
        ("scope", "notifications:read") => "NotificationsRead",
        ("scope", "notifications:write") => "NotificationsWrite",
        ("scope", "channels:write") => "ChannelsWrite",
        ("scope", "governance:read") => "GovernanceRead",
        ("scope", "governance:write") => "GovernanceWrite",
        ("scope", "catalogues:read") => "CataloguesRead",
        ("scope", "catalogues:write") => "CataloguesWrite",
        ("scope", "requests:read") => "RequestsRead",
        ("scope", "requests:write") => "RequestsWrite",
        ("scope", "search:read") => "SearchRead",
        ("scope", "portfolio:read") => "PortfolioRead",
        ("scope", "portfolio:write") => "PortfolioWrite",
        ("scope", "model:read") => "ModelRead",
        ("scope", "model:write") => "ModelWrite",
        // S09. This table is the compile-time twin of `Scope::as_str`; an action naming a scope that is
        // missing here fails the build, which is what keeps the two in step.
        ("scope", "documents:read") => "DocumentsRead",
        ("scope", "documents:write") => "DocumentsWrite",
        ("scope", "secrets:read") => "SecretsRead",
        ("scope", "secrets:write") => "SecretsWrite",
        ("scope", "integrations:read") => "IntegrationsRead",
        ("scope", "integrations:write") => "IntegrationsWrite",
        ("scope", "processes:read") => "ProcessesRead",
        ("scope", "processes:write") => "ProcessesWrite",
        // S17. The benchmark harness's two scopes; no benchmark action is agent-reachable.
        ("scope", "benchmarks:read") => "BenchmarksRead",
        ("scope", "benchmarks:write") => "BenchmarksWrite",
        ("scope", "inference:read") => "InferenceRead",
        ("scope", "inference:write") => "InferenceWrite",
        // S19. The eval instrument's three scopes; `evals:approve` is a human sign-off, so it
        // is granted to Owner|Admin literally (see `Scope::granted_to`), never via `admin`.
        ("scope", "evals:read") => "EvalsRead",
        ("scope", "evals:write") => "EvalsWrite",
        ("scope", "evals:approve") => "EvalsApprove",
        // S13. The workflow definition's authoring pair; publishing is an authoring act, so both
        // are member-and-above (`Scope::granted_to`), and no workflow action is agent-reachable.
        ("scope", "workflows:read") => "WorkflowsRead",
        ("scope", "workflows:write") => "WorkflowsWrite",
        // S15. The capability aggregate's pair: a read is open, a write is member-and-above and
        // is intersected with the caller's own scopes at readiness (CONTRACT §17).
        ("scope", "capabilities:read") => "CapabilitiesRead",
        ("scope", "capabilities:write") => "CapabilitiesWrite",
        // T0. The organisation spine's pair (SPEC_V2 §4): a read is open, a write is platform
        // administration like `departments:write`.
        ("scope", "organisation:read") => "OrganisationRead",
        ("scope", "organisation:write") => "OrganisationWrite",
        (kind, other) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("unknown {kind} {other:?}"),
            ));
        }
    };
    Ok(v.to_owned())
}

/// One declaration → the spec value, the registry submit and the executor wrapper.
#[proc_macro_attribute]
pub fn action(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as ActionArgs);
    let function = parse_macro_input!(item as ItemFn);

    let name = args.name.value();
    if let Some(reason) = name_error(&name) {
        return syn::Error::new(args.name.span(), format!("action name {name:?} {reason}"))
            .to_compile_error()
            .into();
    }

    // The handler contract is fixed: (conn, ctx, input) -> Result<Output, ActionError>.
    // The THIRD parameter's type is the input schema and the Ok type is the output — the
    // macro names neither twice.
    let mut params = function.sig.inputs.iter();
    let _conn = params.next();
    let _ctx = params.next();
    let input_param = match params.next() {
        Some(syn::FnArg::Typed(pat)) => (*pat.ty).clone(),
        _ => {
            return syn::Error::new(
                function.sig.span(),
                "#[action] requires exactly (tx: &mut PgConnection, ctx: HandlerCtx, input: T)",
            )
            .to_compile_error()
            .into();
        }
    };
    if params.next().is_some() {
        return syn::Error::new(
            function.sig.span(),
            "#[action] requires exactly (tx, ctx, input) — a fourth argument would place \
             invocation state outside the one declaration",
        )
        .to_compile_error()
        .into();
    }
    let output_type = match &function.sig.output {
        syn::ReturnType::Type(_, ty) => match ty.as_ref() {
            syn::Type::Path(path) => {
                let segment = path.path.segments.last().expect("a return path");
                if segment.ident != "Result" {
                    return syn::Error::new(
                        ty.span(),
                        "#[action] must return Result<Output, ActionError>",
                    )
                    .to_compile_error()
                    .into();
                }
                match &segment.arguments {
                    syn::PathArguments::AngleBracketed(args) => match args.args.first() {
                        Some(syn::GenericArgument::Type(ty)) => ty.clone(),
                        _ => {
                            return syn::Error::new(ty.span(), "Result needs an Ok type")
                                .to_compile_error()
                                .into();
                        }
                    },
                    _ => {
                        return syn::Error::new(ty.span(), "Result needs an Ok type")
                            .to_compile_error()
                            .into();
                    }
                }
            }
            _ => {
                return syn::Error::new(
                    ty.span(),
                    "#[action] must return Result<Output, ActionError>",
                )
                .to_compile_error()
                .into();
            }
        },
        syn::ReturnType::Default => {
            return syn::Error::new(
                function.sig.span(),
                "#[action] must return Result<Output, ActionError>",
            )
            .to_compile_error()
            .into();
        }
    };

    // `HandlerOutput<T>` is the one Ok type that carries the success status and headers
    // (S05's `cards.create` is `201 + ETag`, `cards.archive` is `204`). Its INNER `T` is the
    // action's output document, and therefore the schema: a projection must describe the
    // action's document, never the macro's transport envelope. Any other Ok type is the
    // document itself and the macro wraps it as `200`.
    let (schema_type, carries_status) = match &output_type {
        syn::Type::Path(path) => match path.path.segments.last() {
            Some(segment) if segment.ident == "HandlerOutput" => match &segment.arguments {
                syn::PathArguments::AngleBracketed(args) => match args.args.first() {
                    Some(syn::GenericArgument::Type(inner)) => (inner.clone(), true),
                    _ => {
                        return syn::Error::new(
                            proc_macro2::Span::call_site(),
                            "#[action] returns HandlerOutput<T>: the Ok type needs an argument",
                        )
                        .to_compile_error()
                        .into();
                    }
                },
                _ => {
                    return syn::Error::new(
                        proc_macro2::Span::call_site(),
                        "#[action] returns HandlerOutput<T>: the Ok type needs an argument",
                    )
                    .to_compile_error()
                    .into();
                }
            },
            _ => (output_type.clone(), false),
        },
        _ => (output_type.clone(), false),
    };

    // The vocabulary spellings become IDENTIFIERS in the expansion; interpolating the
    // String directly would emit a string literal and not compile.
    let kind = match variant_path("kind", &args.kind.value()) {
        Ok(v) => format_ident!("{v}"),
        Err(e) => return e.to_compile_error().into(),
    };
    let effect = match variant_path("effect", &args.effect.value()) {
        Ok(v) => format_ident!("{v}"),
        Err(e) => return e.to_compile_error().into(),
    };
    let confirm = match variant_path("confirm", &args.confirm.value()) {
        Ok(v) => format_ident!("{v}"),
        Err(e) => return e.to_compile_error().into(),
    };
    let tenancy = match variant_path("tenancy", &args.tenancy.value()) {
        Ok(v) => format_ident!("{v}"),
        Err(e) => return e.to_compile_error().into(),
    };
    // S10's copilot hints are optional: an undeclared tier is `AgentTier::None`, and a bad
    // spelling is a compile error like every other vocabulary word here.
    let agent_tier = match &args.agent_tier {
        None => format_ident!("None"),
        Some(tier) => match tier.value().as_str() {
            "none" => format_ident!("None"),
            "core" => format_ident!("Core"),
            "searchable" => format_ident!("Searchable"),
            other => {
                return syn::Error::new(tier.span(), format!("unknown agent_tier {other:?}"))
                    .to_compile_error()
                    .into();
            }
        },
    };
    let proposal_required = args
        .proposal_required
        .as_ref()
        .map(|flag| flag.value())
        .unwrap_or(false);
    let mut scope_paths = Vec::new();
    for scope in &args.scopes {
        match variant_path("scope", &scope.value()) {
            Ok(v) => {
                let ident = format_ident!("{v}");
                scope_paths.push(quote!(::conveyor_actions::Scope::#ident));
            }
            Err(e) => return e.to_compile_error().into(),
        }
    }
    // An empty list emits `&[]` — still a `&'static [Scope]`, never an Option the registry
    // would have to interpret.
    let scopes = quote!(&[#(#scope_paths),*]);

    let ident = &function.sig.ident;
    let handler_ident = format_ident!("{ident}__handler");
    let in_schema_ident = format_ident!("{ident}__input_schema");
    let out_schema_ident = format_ident!("{ident}__output_schema");
    let spec_fn_ident = format_ident!("{ident}__spec");

    // The pair of final expressions: a plain Ok type is the action's document (200, no
    // header); `HandlerOutput<T>` carries its own status and ETag.
    let finish = if carries_status {
        quote! {
            ::conveyor_actions::serde_json::to_value(&produced.value)
                .map(|value| ::conveyor_actions::HandlerOutput {
                    value,
                    status: produced.status,
                    etag: produced.etag,
                })
                .map_err(::conveyor_actions::ActionError::serialize)
        }
    } else {
        quote! {
            ::conveyor_actions::serde_json::to_value(&produced)
                .map(::conveyor_actions::HandlerOutput::ok)
                .map_err(::conveyor_actions::ActionError::serialize)
        }
    };

    let version = args.version;
    let timeout_ms = args.timeout_ms;
    let task_capable = args.task.map(|value| value.value()).unwrap_or(false);
    let captures_usage = args
        .captures_usage
        .map(|value| value.value())
        .unwrap_or(false);
    let idempotent = args.idempotent;
    let name_lit = &args.name;

    let expanded = quote! {
        #function

        #[doc(hidden)]
        fn #in_schema_ident() -> ::conveyor_actions::schemars::Schema {
            ::conveyor_actions::schemars::schema_for!(#input_param)
        }

        #[doc(hidden)]
        fn #out_schema_ident() -> ::conveyor_actions::schemars::Schema {
            ::conveyor_actions::schemars::schema_for!(#schema_type)
        }

        #[doc(hidden)]
        fn #handler_ident<'__a>(
            conn: &'__a mut ::conveyor_actions::sqlx::PgConnection,
            ctx: ::conveyor_actions::HandlerCtx,
            input: ::conveyor_actions::serde_json::Value,
        ) -> ::conveyor_actions::HandlerFuture<'__a> {
            ::std::boxed::Box::pin(async move {
                let typed: #input_param = ::conveyor_actions::serde_json::from_value(input)
                    .map_err(::conveyor_actions::ActionError::from_serde)?;
                let produced = #ident(conn, ctx, typed).await?;
                #finish
            })
        }

        #[doc(hidden)]
        fn #spec_fn_ident() -> ::conveyor_actions::ActionSpec {
            ::conveyor_actions::ActionSpec {
                name: #name_lit,
                version: #version,
                kind: ::conveyor_actions::ActionKind::#kind,
                input: #in_schema_ident,
                output: #out_schema_ident,
                scopes: #scopes,
                idempotent: #idempotent,
                timeout_ms: #timeout_ms,
                effect: ::conveyor_actions::Effect::#effect,
                confirm: ::conveyor_actions::Confirm::#confirm,
                tenancy: ::conveyor_actions::Tenancy::#tenancy,
                agent: ::conveyor_actions::AgentHints {
                    tier: ::conveyor_actions::AgentTier::#agent_tier,
                    proposal_required: #proposal_required,
                },
                task_capable: #task_capable,
                captures_usage: #captures_usage,
                handler: ::conveyor_actions::HandlerRef::Internal(#handler_ident),
            }
        }

        ::conveyor_actions::inventory::submit! {
            ::conveyor_actions::ActionRegistration::new(#spec_fn_ident)
        }
    };

    expanded.into()
}
