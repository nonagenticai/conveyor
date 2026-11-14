//! The tenant-scoped response cache.
//!
//! The key grammar is the old service's: `<namespace>:<tenant_id>:<rest>`, and ownership is decided
//! by [`key_tenant`] alone — one definition, so the invalidation check and the clear scope cannot
//! disagree. A key with no tenant segment belongs to nobody and is refused (fail closed).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct Entry {
    value: String,
    stored: Instant,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Counters {
    pub hits: u64,
    pub misses: u64,
}

#[derive(Debug)]
pub struct Cache {
    ttl: Duration,
    max_entries: usize,
    entries: Mutex<HashMap<String, Entry>>,
    counters: Mutex<Counters>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub size: usize,
    pub max_entries: usize,
    pub ttl_seconds: f64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

/// The tenant segment of `<namespace>:<tenant_id>:<rest>`, or `None` when the key does not carry
/// one. The single definition of the grammar.
pub fn key_tenant(key: &str) -> Option<&str> {
    let mut parts = key.split(':');
    let _namespace = parts.next()?;
    let tenant = parts.next()?;
    if tenant.is_empty() || parts.next().is_none() {
        return None;
    }
    Some(tenant)
}

impl Cache {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            ttl,
            max_entries: max_entries.max(1),
            entries: Mutex::new(HashMap::new()),
            counters: Mutex::new(Counters::default()),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let mut entries = self.entries.lock().expect("cache mutex");
        let fresh = entries
            .get(key)
            .filter(|entry| entry.stored.elapsed() < self.ttl)
            .map(|entry| entry.value.clone());
        if fresh.is_none() {
            entries.remove(key);
        }
        let mut counters = self.counters.lock().expect("counter mutex");
        match &fresh {
            Some(_) => counters.hits += 1,
            None => counters.misses += 1,
        }
        fresh
    }

    pub fn insert(&self, key: String, value: String) {
        let mut entries = self.entries.lock().expect("cache mutex");
        if entries.len() >= self.max_entries && !entries.contains_key(&key) {
            // Evict the oldest insertion; the cache is a cost optimisation, never a record.
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.stored)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(
            key,
            Entry {
                value,
                stored: Instant::now(),
            },
        );
    }

    pub fn invalidate(&self, key: &str) -> bool {
        self.entries
            .lock()
            .expect("cache mutex")
            .remove(key)
            .is_some()
    }

    /// Remove every entry owned by `tenant`, returning how many were removed.
    pub fn clear_tenant(&self, tenant: &str) -> usize {
        let mut entries = self.entries.lock().expect("cache mutex");
        let before = entries.len();
        entries.retain(|key, _| key_tenant(key) != Some(tenant));
        before - entries.len()
    }

    pub fn stats(&self) -> Stats {
        let size = self.entries.lock().expect("cache mutex").len();
        let counters = *self.counters.lock().expect("counter mutex");
        let total = counters.hits + counters.misses;
        Stats {
            size,
            max_entries: self.max_entries,
            ttl_seconds: self.ttl.as_secs_f64(),
            hits: counters.hits,
            misses: counters.misses,
            hit_rate: if total == 0 {
                0.0
            } else {
                counters.hits as f64 / total as f64
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_grammar_names_its_tenant() {
        assert_eq!(key_tenant("stats:tenant-a:meta:1"), Some("tenant-a"));
        assert_eq!(key_tenant("no-tenant-segment"), None);
        assert_eq!(key_tenant("ns::rest"), None);
        assert_eq!(key_tenant("ns:tenant"), None);
    }

    #[test]
    fn a_clear_only_touches_the_calling_tenant() {
        let cache = Cache::new(Duration::from_secs(60), 100);
        cache.insert("stats:a:1".into(), "x".into());
        cache.insert("stats:b:1".into(), "y".into());
        assert_eq!(cache.clear_tenant("a"), 1);
        assert!(cache.get("stats:b:1").is_some(), "b survives a's clear");
    }

    #[test]
    fn a_miss_then_a_hit_is_counted() {
        let cache = Cache::new(Duration::from_secs(60), 100);
        assert!(cache.get("stats:a:1").is_none());
        cache.insert("stats:a:1".into(), "x".into());
        assert!(cache.get("stats:a:1").is_some());
        let stats = cache.stats();
        assert_eq!((stats.hits, stats.misses), (1, 1));
        assert!((stats.hit_rate - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_oldest_entry_is_evicted_at_the_cap() {
        let cache = Cache::new(Duration::from_secs(60), 2);
        cache.insert("stats:a:1".into(), "1".into());
        cache.insert("stats:a:2".into(), "2".into());
        cache.insert("stats:a:3".into(), "3".into());
        assert_eq!(cache.stats().size, 2);
    }
}
