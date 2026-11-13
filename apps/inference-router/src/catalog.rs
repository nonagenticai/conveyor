//! The provider catalog — the live deployment's `providers.yaml` shape.
//!
//! The loader's refusals are part of the contract, not decoration: an alias declared by more than
//! one enabled provider cannot auto-pin, so the request would silently fall through the chain and
//! be answered by whichever provider came first (that is how asking for `deepseek-v4-pro` once
//! returned local Gemma while billing charged DeepSeek rates). A config bug is a boot failure.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

/// The wire protocol a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Openai,
    Anthropic,
}

fn default_timeout() -> f64 {
    600.0
}

/// One provider entry.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderEntry {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    /// Customer tenants may be routed here (free, owner-hosted).
    #[serde(default)]
    pub customer_eligible: bool,
    pub protocol: Protocol,
    #[serde(default)]
    pub endpoint: Option<String>,
    /// The upstream model id used when the request names no alias.
    pub model: String,
    /// An optional explicit model list; empty means "the default model".
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// The owner-hardware credential (e.g. the vLLM licence token).
    #[serde(default)]
    pub upstream_auth_env: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout: f64,
    #[serde(default)]
    pub max_concurrent: Option<usize>,
    #[serde(default)]
    pub cost_input_per_1m: f64,
    #[serde(default)]
    pub cost_output_per_1m: f64,
    /// `json_object` downgrades a `json_schema` request (an upstream that cannot do guided JSON).
    #[serde(default)]
    pub response_format_mode: Option<String>,
    #[serde(default)]
    pub failover_on_validation_failure: bool,
    #[serde(default)]
    pub force_chat_template_kwargs: bool,
    #[serde(default)]
    pub model_aliases: BTreeMap<String, String>,
}

impl ProviderEntry {
    /// Every model id this provider advertises: the default (or explicit list) plus alias keys.
    pub fn model_ids(&self) -> Vec<String> {
        let mut ids = if self.models.is_empty() {
            vec![self.model.clone()]
        } else {
            self.models.clone()
        };
        for alias in self.model_aliases.keys() {
            if !ids.contains(alias) {
                ids.push(alias.clone());
            }
        }
        ids
    }
}

/// The whole catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    pub providers: Vec<ProviderEntry>,
    pub default_chain: Vec<String>,
    /// Aliases a failover chain legitimately shares (they ride `default_chain`, not a pin).
    #[serde(default)]
    pub shared_aliases: Vec<String>,
    /// Aliases that must never fail over (one owner, never shared).
    #[serde(default)]
    pub no_failover_aliases: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot parse {path}: {source}")]
    Parse {
        path: String,
        source: Box<serde_yaml::Error>,
    },
    #[error(
        "alias '{alias}' is declared by more than one enabled provider ({owners}) and is not in shared_aliases"
    )]
    AmbiguousAlias { alias: String, owners: String },
    #[error("shared_aliases entry '{alias}' is not declared by two or more enabled providers")]
    StaleSharedAlias { alias: String },
    #[error(
        "no_failover_aliases entry '{alias}' has {count} declaring provider(s); it must have exactly one"
    )]
    NoFailoverOwner { alias: String, count: usize },
    #[error(
        "no_failover_aliases entry '{alias}' is in shared_aliases: it would ride the default chain"
    )]
    NoFailoverShared { alias: String },
    #[error(
        "provider '{name}' is customer_eligible but priced or keyed; only a free owner-hosted provider may be customer-eligible"
    )]
    CommercialProviderEligible { name: String },
}

impl Catalog {
    pub fn load(path: &Path) -> Result<Self, CatalogError> {
        let text = std::fs::read_to_string(path).map_err(|source| CatalogError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_yaml(&text).map_err(|error| match error {
            CatalogError::Parse { source, .. } => CatalogError::Parse {
                path: path.display().to_string(),
                source,
            },
            other => other,
        })
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, CatalogError> {
        let catalog: Catalog =
            serde_yaml::from_str(yaml).map_err(|source| CatalogError::Parse {
                path: "providers.yaml".to_owned(),
                source: Box::new(source),
            })?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// The boot-time refusals, in the order the live loader applies them.
    pub fn validate(&self) -> Result<(), CatalogError> {
        let mut owners: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for provider in self.providers.iter().filter(|provider| provider.enabled) {
            for alias in provider.model_aliases.keys() {
                owners
                    .entry(alias.as_str())
                    .or_default()
                    .insert(provider.name.as_str());
            }
        }

        let shared: BTreeSet<&str> = self.shared_aliases.iter().map(String::as_str).collect();
        for (alias, owner_names) in &owners {
            if owner_names.len() > 1 && !shared.contains(alias) {
                return Err(CatalogError::AmbiguousAlias {
                    alias: (*alias).to_owned(),
                    owners: owner_names.iter().copied().collect::<Vec<_>>().join(", "),
                });
            }
        }
        for alias in &self.shared_aliases {
            let count = owners.get(alias.as_str()).map_or(0, BTreeSet::len);
            if count < 2 {
                return Err(CatalogError::StaleSharedAlias {
                    alias: alias.clone(),
                });
            }
        }

        for alias in &self.no_failover_aliases {
            if shared.contains(alias.as_str()) {
                return Err(CatalogError::NoFailoverShared {
                    alias: alias.clone(),
                });
            }
            let count = self
                .providers
                .iter()
                .filter(|provider| provider.model_aliases.contains_key(alias))
                .count();
            if count != 1 {
                return Err(CatalogError::NoFailoverOwner {
                    alias: alias.clone(),
                    count,
                });
            }
        }

        for provider in self.providers.iter().filter(|provider| provider.enabled) {
            if provider.customer_eligible
                && (provider.cost_input_per_1m != 0.0
                    || provider.cost_output_per_1m != 0.0
                    || provider.api_key_env.is_some())
            {
                return Err(CatalogError::CommercialProviderEligible {
                    name: provider.name.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn enabled(&self) -> impl Iterator<Item = &ProviderEntry> {
        self.providers.iter().filter(|provider| provider.enabled)
    }

    pub fn provider(&self, name: &str) -> Option<&ProviderEntry> {
        self.enabled().find(|provider| provider.name == name)
    }

    /// The single enabled provider that declares `alias`, or `None` (unknown or ambiguous).
    pub fn alias_owner(&self, alias: &str) -> Option<&str> {
        let mut owner = None;
        for provider in self.enabled() {
            if provider.model_aliases.contains_key(alias) {
                if owner.is_some() {
                    return None;
                }
                owner = Some(provider.name.as_str());
            }
        }
        owner
    }

    /// Whether ANY provider (enabled or not) declares the alias — the distinction between
    /// "unknown name, use the chain" and "a catalog alias nothing can serve now".
    pub fn any_provider_declares(&self, alias: &str) -> bool {
        self.providers
            .iter()
            .any(|provider| provider.model_aliases.contains_key(alias))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(yaml: &str) -> Result<Catalog, CatalogError> {
        Catalog::from_yaml(yaml)
    }

    const BASE: &str = r#"
providers:
  - name: pgx-local
    enabled: true
    customer_eligible: true
    protocol: openai
    endpoint: http://127.0.0.1:8001/v1
    model: local-llm
    model_aliases:
      local-llm: local-llm
      tier-high: local-llm
  - name: deepseek
    enabled: false
    protocol: openai
    endpoint: https://api.example.com/v1
    model: deepseek-v4-flash
    api_key_env: DEEPSEEK_KEY
    model_aliases:
      local-llm: deepseek-v4-flash
default_chain: [pgx-local, deepseek]
"#;

    #[test]
    fn a_valid_catalog_loads_and_resolves_aliases() {
        let catalog = catalog(BASE).expect("valid");
        assert_eq!(catalog.alias_owner("local-llm"), Some("pgx-local"));
        assert_eq!(catalog.alias_owner("tier-high"), Some("pgx-local"));
        assert_eq!(catalog.alias_owner("nope"), None);
        // The disabled provider still declares local-llm, so the name is a catalog alias.
        assert!(catalog.any_provider_declares("local-llm"));
    }

    #[test]
    fn an_alias_owned_by_two_enabled_providers_is_a_boot_failure() {
        let yaml = BASE.replace("enabled: false", "enabled: true");
        let error = catalog(&yaml).expect_err("ambiguous alias");
        assert!(
            matches!(error, CatalogError::AmbiguousAlias { ref alias, .. } if alias == "local-llm"),
            "{error}"
        );
    }

    #[test]
    fn a_shared_alias_that_only_one_provider_declares_is_a_boot_failure() {
        let yaml = format!("{BASE}shared_aliases: [tier-high]\n");
        let error = catalog(&yaml).expect_err("stale shared alias");
        assert!(
            matches!(error, CatalogError::StaleSharedAlias { .. }),
            "{error}"
        );
    }

    #[test]
    fn a_shared_alias_is_allowed_when_two_enabled_providers_declare_it() {
        let yaml = format!(
            "{}{}",
            BASE.replace("enabled: false", "enabled: true"),
            "shared_aliases: [local-llm]\n"
        );
        let catalog = catalog(&yaml).expect("deliberate sharing");
        // Shared means it must NOT auto-pin.
        assert_eq!(catalog.alias_owner("local-llm"), None);
    }

    #[test]
    fn a_no_failover_alias_must_have_exactly_one_owner() {
        let yaml = format!("{BASE}no_failover_aliases: [ghost]\n");
        let error = catalog(&yaml).expect_err("nobody declares it");
        assert!(
            matches!(error, CatalogError::NoFailoverOwner { count: 0, .. }),
            "{error}"
        );

        let yaml = format!("{BASE}no_failover_aliases: [local-llm]\n");
        let error = catalog(&yaml).expect_err("two providers declare it");
        assert!(
            matches!(error, CatalogError::NoFailoverOwner { count: 2, .. }),
            "{error}"
        );
    }

    #[test]
    fn a_priced_or_keyed_customer_eligible_provider_is_a_boot_failure() {
        let yaml = r#"
providers:
  - name: paid
    enabled: true
    customer_eligible: true
    protocol: openai
    endpoint: https://api.example.com/v1
    model: m
    api_key_env: PAID_KEY
default_chain: [paid]
"#;
        let error = catalog(yaml).expect_err("commercial provider marked eligible");
        assert!(
            matches!(error, CatalogError::CommercialProviderEligible { ref name } if name == "paid"),
            "{error}"
        );
    }

    #[test]
    fn model_ids_lists_the_default_and_the_aliases() {
        let catalog = catalog(BASE).expect("valid");
        let pgx = catalog.provider("pgx-local").expect("enabled");
        assert_eq!(pgx.model_ids(), vec!["local-llm", "tier-high"]);
    }
}
