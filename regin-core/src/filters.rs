//! Notice filters (FEAT-052 / DISC-015).
//!
//! Hand-editable rule files in a dedicated store (separate from `desired/`),
//! layered user-over-system. Filters drop known-noise observations *before* they
//! reach the LLM review tier (FEAT-049), cutting evaluation cost without losing
//! real signal. Every drop is measured by the notice-filter-savings KPI
//! ([`crate::kpi::M_NOTICE_FILTER_SAVED`]).

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tracing::warn;

use crate::kpi;

/// Where a filter rule came from (user overrides system by name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterSource {
    System,
    User,
}

impl std::fmt::Display for FilterSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterSource::System => write!(f, "system"),
            FilterSource::User => write!(f, "user"),
        }
    }
}

/// A single noise-suppression rule. An observation is filtered when its text
/// contains `contains` (case-insensitive) and — if `domain` is set — comes from
/// that domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterRule {
    pub name: String,
    pub contains: String,
    pub domain: Option<String>,
    pub source: FilterSource,
}

#[derive(Deserialize)]
struct RulesToml {
    #[serde(default, rename = "rule")]
    rules: Vec<RuleToml>,
}

#[derive(Deserialize)]
struct RuleToml {
    name: String,
    contains: String,
    domain: Option<String>,
}

/// Parse a filters file's TOML content into rules.
pub fn parse_rules(content: &str, source: FilterSource) -> Result<Vec<FilterRule>> {
    let parsed: RulesToml = toml::from_str(content).context("parsing notice-filter rules")?;
    Ok(parsed
        .rules
        .into_iter()
        .map(|r| FilterRule {
            name: r.name,
            contains: r.contains,
            domain: r.domain,
            source,
        })
        .collect())
}

fn load_rules_from_dir(dir: &Path, source: FilterSource) -> Vec<FilterRule> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(c) => match parse_rules(&c, source) {
                Ok(mut rules) => out.append(&mut rules),
                Err(e) => warn!("skipping malformed filter file {}: {e:#}", path.display()),
            },
            Err(e) => warn!("skipping unreadable filter file {}: {e}", path.display()),
        }
    }
    out
}

/// Load all filter rules, with user rules overriding system rules of the same
/// name and adding any new ones (fail-safe: malformed files are skipped).
pub fn load_filters(system_dir: &Path, user_dir: &Path) -> Vec<FilterRule> {
    let mut by_name: BTreeMap<String, FilterRule> = BTreeMap::new();
    for r in load_rules_from_dir(system_dir, FilterSource::System) {
        by_name.insert(r.name.clone(), r);
    }
    for r in load_rules_from_dir(user_dir, FilterSource::User) {
        by_name.insert(r.name.clone(), r);
    }
    by_name.into_values().collect()
}

/// The first rule that filters an observation from `domain` with text `text`.
pub fn first_match<'a>(rules: &'a [FilterRule], domain: &str, text: &str) -> Option<&'a FilterRule> {
    let lower = text.to_lowercase();
    rules.iter().find(|r| {
        r.domain.as_deref().map(|d| d == domain).unwrap_or(true)
            && lower.contains(&r.contains.to_lowercase())
    })
}

/// Whether an observation is filtered (dropped) by any rule.
pub fn is_filtered(rules: &[FilterRule], domain: &str, text: &str) -> bool {
    first_match(rules, domain, text).is_some()
}

/// Apply filters to an observation; if dropped, record the notice-filter-savings
/// KPI and return `true`. Used by the evaluation tier before the LLM call.
pub fn filter_and_record(
    conn: &Connection,
    rules: &[FilterRule],
    domain: &str,
    text: &str,
) -> Result<bool> {
    match first_match(rules, domain, text) {
        Some(rule) => {
            kpi::kpi_record(conn, kpi::M_NOTICE_FILTER_SAVED, 1.0, Some(&rule.name))?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::path::PathBuf;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("regin-filters-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn parses_and_matches_case_insensitively_with_domain_scope() {
        let rules = parse_rules(
            "[[rule]]\nname=\"debug\"\ncontains=\"DEBUG\"\n\n[[rule]]\nname=\"net-blip\"\ncontains=\"transient\"\ndomain=\"network\"\n",
            FilterSource::System,
        )
        .unwrap();
        assert_eq!(rules.len(), 2);

        // unscoped rule matches any domain, case-insensitively
        assert!(is_filtered(&rules, "logs", "a debug line"));
        assert!(is_filtered(&rules, "anything", "DEBUG: x"));
        // scoped rule only matches its domain
        assert!(is_filtered(&rules, "network", "transient blip"));
        assert!(!is_filtered(&rules, "disk", "transient blip"), "domain-scoped rule does not match other domains");
        // non-matching text passes through
        assert!(!is_filtered(&rules, "logs", "all good"));
    }

    #[test]
    fn user_rules_override_and_add() {
        let sys = tmpdir();
        let user = tmpdir();
        fs::write(sys.join("base.toml"), "[[rule]]\nname=\"a\"\ncontains=\"sys-text\"\n").unwrap();
        fs::write(user.join("more.toml"), "[[rule]]\nname=\"a\"\ncontains=\"user-text\"\n[[rule]]\nname=\"b\"\ncontains=\"extra\"\n").unwrap();

        let rules = load_filters(&sys, &user);
        let a = rules.iter().find(|r| r.name == "a").unwrap();
        assert_eq!(a.contains, "user-text", "user overrides system rule of same name");
        assert_eq!(a.source, FilterSource::User);
        assert!(rules.iter().any(|r| r.name == "b"), "user adds new rule");
        assert_eq!(rules.len(), 2);

        fs::remove_dir_all(&sys).ok();
        fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn malformed_file_is_skipped() {
        let sys = tmpdir();
        let user = tmpdir();
        fs::write(sys.join("ok.toml"), "[[rule]]\nname=\"a\"\ncontains=\"x\"\n").unwrap();
        fs::write(sys.join("bad.toml"), "this is not [valid toml").unwrap();
        let rules = load_filters(&sys, &user);
        assert_eq!(rules.len(), 1, "bad file skipped, good one loads");
        fs::remove_dir_all(&sys).ok();
        fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn filter_and_record_emits_kpi_only_on_drop() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        let rules = parse_rules("[[rule]]\nname=\"debug\"\ncontains=\"debug\"\n", FilterSource::System).unwrap();
        let epoch = "1970-01-01T00:00:00Z";

        assert!(filter_and_record(&conn, &rules, "logs", "a debug entry").unwrap());
        assert!(!filter_and_record(&conn, &rules, "logs", "real error").unwrap());
        assert_eq!(kpi::kpi_count(&conn, kpi::M_NOTICE_FILTER_SAVED, epoch).unwrap(), 1, "only the dropped notice is counted");
    }
}
