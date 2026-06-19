//! Desired-state ("to-be") model (FEAT-033 / DISC-008).
//!
//! Each domain (disk, services, …) has a per-domain markdown file carrying two
//! layers: a free-text **intent** (prose) and a fenced ` ```assertions ` block of
//! machine-checkable TOML. Files are layered **user-over-system** like skills.
//!
//! The loader is fail-safe: a malformed file is logged and skipped so one bad
//! file never takes monitoring down (callers retain the last good set). When a
//! domain's structured target is internally **contradictory** (an ambiguous
//! to-be state), [`check_and_open_problems`] opens a *problem* — not an incident —
//! because resolving the ambiguity needs a human (DISC-008).

use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::db;

/// Where a desired-state file came from (user overrides system).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesiredSource {
    System,
    User,
}

impl std::fmt::Display for DesiredSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DesiredSource::System => write!(f, "system"),
            DesiredSource::User => write!(f, "user"),
        }
    }
}

/// A comparison operator in an assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssertOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl AssertOp {
    fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "lt" | "<" => AssertOp::Lt,
            "le" | "lte" | "<=" => AssertOp::Le,
            "gt" | ">" => AssertOp::Gt,
            "ge" | "gte" | ">=" => AssertOp::Ge,
            "eq" | "==" | "=" => AssertOp::Eq,
            "ne" | "!=" | "<>" => AssertOp::Ne,
            other => bail!("unknown assertion operator: {other:?}"),
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AssertOp::Lt => "lt",
            AssertOp::Le => "le",
            AssertOp::Gt => "gt",
            AssertOp::Ge => "ge",
            AssertOp::Eq => "eq",
            AssertOp::Ne => "ne",
        }
    }
}

/// The target value of an assertion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssertValue {
    Num(f64),
    Text(String),
}

impl std::fmt::Display for AssertValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssertValue::Num(n) => write!(f, "{n}"),
            AssertValue::Text(t) => write!(f, "{t:?}"),
        }
    }
}

/// One machine-checkable assertion about a domain's observed signals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assertion {
    pub key: String,
    pub op: AssertOp,
    pub value: AssertValue,
    pub description: Option<String>,
}

impl std::fmt::Display for Assertion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} {}", self.key, self.op.as_str(), self.value)
    }
}

/// A domain's loaded desired ("to-be") state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesiredState {
    pub domain: String,
    /// Free-text intent (everything outside the assertions block).
    pub intent: String,
    pub assertions: Vec<Assertion>,
    /// Per-domain override for the recurrence→problem threshold (FEAT-036).
    pub recurrence_threshold: Option<usize>,
    /// Per-domain monitor cadence tune (FEAT-047), e.g. `hourly` / `every 15m`.
    pub cadence: Option<String>,
    pub source: DesiredSource,
    pub path: PathBuf,
}

/// A compact summary for listing (FEAT-033 CLI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesiredInfo {
    pub domain: String,
    pub source: DesiredSource,
    pub assertions: usize,
    pub recurrence_threshold: Option<usize>,
    /// Non-empty when the target is self-contradictory (ambiguous).
    pub conflicts: Vec<String>,
}

// --- TOML shape of the assertions block ---

#[derive(Deserialize)]
struct AssertionsToml {
    recurrence_threshold: Option<usize>,
    cadence: Option<String>,
    #[serde(default, rename = "assert")]
    asserts: Vec<AssertToml>,
}

#[derive(Deserialize)]
struct AssertToml {
    key: String,
    op: String,
    value: toml::Value,
    description: Option<String>,
}

fn assert_value_from_toml(v: toml::Value) -> Result<AssertValue> {
    Ok(match v {
        toml::Value::Integer(i) => AssertValue::Num(i as f64),
        toml::Value::Float(f) => AssertValue::Num(f),
        toml::Value::String(s) => AssertValue::Text(s),
        toml::Value::Boolean(b) => AssertValue::Text(b.to_string()),
        other => bail!("unsupported assertion value: {other:?}"),
    })
}

/// Split a desired-state file into (intent, optional assertions-block source).
/// Only the first ` ```assertions ` fenced block is treated as structured; any
/// later fences stay in the intent.
fn split_assertions_block(content: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut intent: Vec<&str> = Vec::new();
    let mut block: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        let is_open = block.is_none()
            && trimmed.starts_with("```")
            && trimmed
                .trim_start_matches('`')
                .trim()
                .eq_ignore_ascii_case("assertions");
        if is_open {
            i += 1;
            let mut body: Vec<&str> = Vec::new();
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                body.push(lines[i]);
                i += 1;
            }
            block = Some(body.join("\n"));
            i += 1; // skip closing fence (if present)
            continue;
        }
        intent.push(lines[i]);
        i += 1;
    }
    (intent.join("\n").trim().to_string(), block)
}

/// Parse a desired-state file's content into a [`DesiredState`].
pub fn parse_desired_state(
    domain: &str,
    content: &str,
    source: DesiredSource,
    path: PathBuf,
) -> Result<DesiredState> {
    let (intent, block) = split_assertions_block(content);
    let (assertions, recurrence_threshold, cadence) = match block {
        Some(src) => {
            let parsed: AssertionsToml = toml::from_str(&src)
                .with_context(|| format!("invalid assertions block in desired state `{domain}`"))?;
            let mut asserts = Vec::with_capacity(parsed.asserts.len());
            for a in parsed.asserts {
                asserts.push(Assertion {
                    op: AssertOp::parse(&a.op)
                        .with_context(|| format!("assertion `{}` in `{domain}`", a.key))?,
                    value: assert_value_from_toml(a.value)
                        .with_context(|| format!("assertion `{}` in `{domain}`", a.key))?,
                    key: a.key,
                    description: a.description,
                });
            }
            (asserts, parsed.recurrence_threshold, parsed.cadence)
        }
        None => (Vec::new(), None, None),
    };
    Ok(DesiredState {
        domain: domain.to_string(),
        intent,
        assertions,
        recurrence_threshold,
        cadence,
        source,
        path,
    })
}

/// Load every desired-state from one directory (`*.md`), skipping malformed files
/// (fail-safe). The domain is the file stem.
fn load_desired_from_dir(dir: &Path, source: DesiredSource) -> Vec<DesiredState> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out, // missing dir is fine
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let domain = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                warn!("skipping unreadable desired-state {}: {e}", path.display());
                continue;
            }
        };
        match parse_desired_state(&domain, &content, source, path.clone()) {
            Ok(ds) => out.push(ds),
            Err(e) => warn!("skipping malformed desired-state {}: {e:#}", path.display()),
        }
    }
    out
}

/// Load all desired states, user overriding system by domain (FEAT-033).
pub fn load_all_desired(system_dir: &Path, user_dir: &Path) -> Vec<DesiredState> {
    let mut by_domain: BTreeMap<String, DesiredState> = BTreeMap::new();
    for ds in load_desired_from_dir(system_dir, DesiredSource::System) {
        by_domain.insert(ds.domain.clone(), ds);
    }
    for ds in load_desired_from_dir(user_dir, DesiredSource::User) {
        by_domain.insert(ds.domain.clone(), ds);
    }
    by_domain.into_values().collect()
}

/// Load a single domain's desired state, preferring the user file (FEAT-033).
pub fn load_desired(
    system_dir: &Path,
    user_dir: &Path,
    domain: &str,
) -> Result<Option<DesiredState>> {
    let user_path = user_dir.join(format!("{domain}.md"));
    if user_path.is_file() {
        let content = fs::read_to_string(&user_path)?;
        return Ok(Some(parse_desired_state(
            domain,
            &content,
            DesiredSource::User,
            user_path,
        )?));
    }
    let sys_path = system_dir.join(format!("{domain}.md"));
    if sys_path.is_file() {
        let content = fs::read_to_string(&sys_path)?;
        return Ok(Some(parse_desired_state(
            domain,
            &content,
            DesiredSource::System,
            sys_path,
        )?));
    }
    Ok(None)
}

/// The effective recurrence→problem threshold for a domain (FEAT-036): the
/// per-domain to-be-state override if present, else the global `default`.
/// Fail-safe — a missing/unreadable file yields the default.
pub fn recurrence_threshold(
    system_dir: &Path,
    user_dir: &Path,
    domain: &str,
    default: usize,
) -> usize {
    match load_desired(system_dir, user_dir, domain) {
        Ok(Some(ds)) => ds.recurrence_threshold.unwrap_or(default),
        _ => default,
    }
}

/// The per-domain cadence tune from the to-be-state doc, if any (FEAT-047).
/// Fail-safe — a missing/unreadable file yields `None`.
pub fn cadence_tune(system_dir: &Path, user_dir: &Path, domain: &str) -> Option<String> {
    match load_desired(system_dir, user_dir, domain) {
        Ok(Some(ds)) => ds.cadence,
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Contradiction detection (the "ambiguous target -> problem" check)
// ---------------------------------------------------------------------------

/// Human-readable reasons a desired state's target is self-contradictory. An
/// empty result means the structured target is internally consistent.
pub fn contradictions(ds: &DesiredState) -> Vec<String> {
    let mut by_key: BTreeMap<&str, Vec<&Assertion>> = BTreeMap::new();
    for a in &ds.assertions {
        by_key.entry(a.key.as_str()).or_default().push(a);
    }
    by_key
        .into_iter()
        .filter_map(|(key, asserts)| key_contradiction(key, &asserts))
        .collect()
}

fn key_contradiction(key: &str, asserts: &[&Assertion]) -> Option<String> {
    let has_num = asserts.iter().any(|a| matches!(a.value, AssertValue::Num(_)));
    let has_text = asserts.iter().any(|a| matches!(a.value, AssertValue::Text(_)));
    if has_num && has_text {
        return Some(format!("`{key}` mixes numeric and text targets"));
    }
    if has_num {
        numeric_contradiction(key, asserts)
    } else {
        text_contradiction(key, asserts)
    }
}

/// Tighten a lower bound to the greatest one seen (strict wins ties).
fn tighten_lower(cur: &mut Option<(f64, bool)>, v: f64, strict: bool) {
    match cur {
        None => *cur = Some((v, strict)),
        Some((cv, cs)) => {
            if v > *cv || (v == *cv && strict && !*cs) {
                *cur = Some((v, strict));
            }
        }
    }
}

/// Tighten an upper bound to the least one seen (strict wins ties).
fn tighten_upper(cur: &mut Option<(f64, bool)>, v: f64, strict: bool) {
    match cur {
        None => *cur = Some((v, strict)),
        Some((cv, cs)) => {
            if v < *cv || (v == *cv && strict && !*cs) {
                *cur = Some((v, strict));
            }
        }
    }
}

fn numeric_contradiction(key: &str, asserts: &[&Assertion]) -> Option<String> {
    let mut lower: Option<(f64, bool)> = None;
    let mut upper: Option<(f64, bool)> = None;
    let mut eqs: Vec<f64> = Vec::new();
    let mut nes: Vec<f64> = Vec::new();
    for a in asserts {
        let v = match a.value {
            AssertValue::Num(n) => n,
            AssertValue::Text(_) => continue,
        };
        match a.op {
            AssertOp::Gt => tighten_lower(&mut lower, v, true),
            AssertOp::Ge => tighten_lower(&mut lower, v, false),
            AssertOp::Lt => tighten_upper(&mut upper, v, true),
            AssertOp::Le => tighten_upper(&mut upper, v, false),
            AssertOp::Eq => eqs.push(v),
            AssertOp::Ne => nes.push(v),
        }
    }

    if let (Some((l, ls)), Some((u, us))) = (lower, upper)
        && (l > u || (l == u && (ls || us)))
    {
        return Some(format!("`{key}` has an unsatisfiable range (> {l}, < {u})"));
    }

    if let Some(&e0) = eqs.first() {
        if eqs.iter().any(|e| (*e - e0).abs() > f64::EPSILON) {
            return Some(format!("`{key}` must equal multiple different values"));
        }
        if let Some((l, ls)) = lower
            && (e0 < l || (e0 == l && ls))
        {
            return Some(format!("`{key}` equals {e0} but must be greater than {l}"));
        }
        if let Some((u, us)) = upper
            && (e0 > u || (e0 == u && us))
        {
            return Some(format!("`{key}` equals {e0} but must be less than {u}"));
        }
        if nes.iter().any(|n| (*n - e0).abs() <= f64::EPSILON) {
            return Some(format!("`{key}` both equals and does not equal {e0}"));
        }
    }
    None
}

fn text_contradiction(key: &str, asserts: &[&Assertion]) -> Option<String> {
    let mut eqs: Vec<&str> = Vec::new();
    let mut nes: Vec<&str> = Vec::new();
    for a in asserts {
        let t = match &a.value {
            AssertValue::Text(t) => t.as_str(),
            AssertValue::Num(_) => continue,
        };
        match a.op {
            AssertOp::Eq => eqs.push(t),
            AssertOp::Ne => nes.push(t),
            _ => return Some(format!("`{key}` uses an ordering operator on a text value")),
        }
    }
    if let Some(&first) = eqs.first() {
        if eqs.iter().any(|e| *e != first) {
            return Some(format!("`{key}` must equal multiple different text values"));
        }
        if nes.contains(&first) {
            return Some(format!("`{key}` both equals and does not equal {first:?}"));
        }
    }
    None
}

/// For each desired state with a contradictory target, open a *problem*
/// (idempotent by title) so a human resolves the ambiguity. Consistent states are
/// a no-op. Returns the domains found in conflict (DISC-008/FEAT-033).
pub fn check_and_open_problems(conn: &Connection, states: &[DesiredState]) -> Result<Vec<String>> {
    let mut conflicted = Vec::new();
    for ds in states {
        let reasons = contradictions(ds);
        if reasons.is_empty() {
            continue;
        }
        conflicted.push(ds.domain.clone());
        let title = format!("desired-state conflict: {}", ds.domain);
        let already = db::problem_list(conn, Some("open"))?
            .into_iter()
            .any(|p| p.title == title);
        if !already {
            let desc = format!(
                "The to-be state for `{}` is ambiguous/contradictory and needs a human:\n- {}",
                ds.domain,
                reasons.join("\n- ")
            );
            db::problem_open(conn, &title, &desc)?;
        }
    }
    Ok(conflicted)
}

/// Build listing summaries (with conflict flags) for a set of states.
pub fn summaries(states: &[DesiredState]) -> Vec<DesiredInfo> {
    states
        .iter()
        .map(|ds| DesiredInfo {
            domain: ds.domain.clone(),
            source: ds.source,
            assertions: ds.assertions.len(),
            recurrence_threshold: ds.recurrence_threshold,
            conflicts: contradictions(ds),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("regin-desired-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    const DISK_MD: &str = "\
# Disk

Root and data volumes should keep comfortable free space.

```assertions
recurrence_threshold = 5

[[assert]]
key = \"disk.root.use_percent\"
op = \"lt\"
value = 90

[[assert]]
key = \"disk.fs.mode\"
op = \"eq\"
value = \"rw\"
description = \"filesystems stay writable\"
```

Anything above is a deviation worth attention.
";

    fn parse(content: &str) -> DesiredState {
        parse_desired_state("disk", content, DesiredSource::System, PathBuf::from("disk.md")).unwrap()
    }

    #[test]
    fn parses_intent_assertions_and_threshold() {
        let ds = parse(DISK_MD);
        assert!(ds.intent.contains("comfortable free space"));
        assert!(ds.intent.contains("deviation worth attention"));
        assert!(!ds.intent.contains("[[assert]]"), "structured block excluded from intent");
        assert_eq!(ds.recurrence_threshold, Some(5));
        assert_eq!(ds.cadence, None);
        assert_eq!(ds.assertions.len(), 2);
        assert_eq!(ds.assertions[0].key, "disk.root.use_percent");
        assert_eq!(ds.assertions[0].op, AssertOp::Lt);
        assert_eq!(ds.assertions[0].value, AssertValue::Num(90.0));
        assert_eq!(ds.assertions[1].value, AssertValue::Text("rw".into()));
        assert_eq!(ds.assertions[1].description.as_deref(), Some("filesystems stay writable"));
    }

    #[test]
    fn intent_only_file_has_no_assertions() {
        let ds = parse("# Notes\n\nKeep things tidy.\n");
        assert!(ds.assertions.is_empty());
        assert_eq!(ds.recurrence_threshold, None);
        assert!(ds.intent.contains("Keep things tidy"));
    }

    #[test]
    fn op_aliases_parse() {
        assert_eq!(AssertOp::parse(">=").unwrap(), AssertOp::Ge);
        assert_eq!(AssertOp::parse("LTE").unwrap(), AssertOp::Le);
        assert_eq!(AssertOp::parse("!=").unwrap(), AssertOp::Ne);
        assert!(AssertOp::parse("approximately").is_err());
    }

    #[test]
    fn malformed_assertions_block_errors() {
        let bad = "# x\n\n```assertions\nthis is not = valid = toml\n```\n";
        assert!(parse_desired_state("x", bad, DesiredSource::System, PathBuf::new()).is_err());
    }

    #[test]
    fn user_overrides_system_by_domain() {
        let sys = tmpdir();
        let user = tmpdir();
        fs::write(sys.join("disk.md"), "# sys\n\n```assertions\n[[assert]]\nkey=\"a\"\nop=\"lt\"\nvalue=1\n```\n").unwrap();
        fs::write(user.join("disk.md"), "# user\n\n```assertions\n[[assert]]\nkey=\"a\"\nop=\"lt\"\nvalue=2\n```\n").unwrap();
        fs::write(sys.join("net.md"), "# net only in system\n").unwrap();

        let all = load_all_desired(&sys, &user);
        let disk = all.iter().find(|d| d.domain == "disk").unwrap();
        assert_eq!(disk.source, DesiredSource::User);
        assert_eq!(disk.assertions[0].value, AssertValue::Num(2.0));
        assert!(all.iter().any(|d| d.domain == "net" && d.source == DesiredSource::System));

        // load_desired prefers user too
        assert_eq!(load_desired(&sys, &user, "disk").unwrap().unwrap().source, DesiredSource::User);
        assert_eq!(load_desired(&sys, &user, "net").unwrap().unwrap().source, DesiredSource::System);
        assert!(load_desired(&sys, &user, "absent").unwrap().is_none());

        fs::remove_dir_all(&sys).ok();
        fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn malformed_file_is_skipped_good_files_load() {
        let sys = tmpdir();
        let user = tmpdir();
        fs::write(sys.join("good.md"), "# good\n\n```assertions\n[[assert]]\nkey=\"a\"\nop=\"lt\"\nvalue=1\n```\n").unwrap();
        fs::write(sys.join("bad.md"), "# bad\n\n```assertions\n!!! not toml !!!\n```\n").unwrap();

        let all = load_all_desired(&sys, &user);
        assert_eq!(all.len(), 1, "the bad file is skipped, the good one loads");
        assert_eq!(all[0].domain, "good");

        fs::remove_dir_all(&sys).ok();
        fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn consistent_target_has_no_contradictions() {
        let ds = parse(DISK_MD);
        assert!(contradictions(&ds).is_empty());
    }

    #[test]
    fn detects_numeric_contradictions() {
        // empty range
        let ds = parse("# x\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"gt\"\nvalue=90\n[[assert]]\nkey=\"k\"\nop=\"lt\"\nvalue=80\n```\n");
        assert_eq!(contradictions(&ds).len(), 1);
        // eq outside range
        let ds = parse("# x\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"lt\"\nvalue=10\n[[assert]]\nkey=\"k\"\nop=\"eq\"\nvalue=20\n```\n");
        assert_eq!(contradictions(&ds).len(), 1);
        // two different equalities
        let ds = parse("# x\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"eq\"\nvalue=1\n[[assert]]\nkey=\"k\"\nop=\"eq\"\nvalue=2\n```\n");
        assert_eq!(contradictions(&ds).len(), 1);
        // eq and ne the same value
        let ds = parse("# x\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"eq\"\nvalue=5\n[[assert]]\nkey=\"k\"\nop=\"ne\"\nvalue=5\n```\n");
        assert_eq!(contradictions(&ds).len(), 1);
        // satisfiable range with eq inside -> none
        let ds = parse("# x\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"ge\"\nvalue=0\n[[assert]]\nkey=\"k\"\nop=\"le\"\nvalue=10\n[[assert]]\nkey=\"k\"\nop=\"eq\"\nvalue=5\n```\n");
        assert!(contradictions(&ds).is_empty());
    }

    #[test]
    fn detects_text_and_type_contradictions() {
        // mixed numeric + text on one key
        let ds = parse("# x\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"lt\"\nvalue=1\n[[assert]]\nkey=\"k\"\nop=\"eq\"\nvalue=\"on\"\n```\n");
        assert_eq!(contradictions(&ds).len(), 1);
        // contradictory text equalities
        let ds = parse("# x\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"eq\"\nvalue=\"on\"\n[[assert]]\nkey=\"k\"\nop=\"eq\"\nvalue=\"off\"\n```\n");
        assert_eq!(contradictions(&ds).len(), 1);
        // ordering op on text
        let ds = parse("# x\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"lt\"\nvalue=\"on\"\n```\n");
        assert_eq!(contradictions(&ds).len(), 1);
    }

    #[test]
    fn conflict_opens_problem_idempotently_agreement_does_not() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();

        let good = parse(DISK_MD);
        let bad = parse("# bad\n\n```assertions\n[[assert]]\nkey=\"k\"\nop=\"gt\"\nvalue=9\n[[assert]]\nkey=\"k\"\nop=\"lt\"\nvalue=1\n```\n");
        let mut bad = bad;
        bad.domain = "memory".into();

        // good alone -> no problem
        assert!(check_and_open_problems(&conn, std::slice::from_ref(&good)).unwrap().is_empty());
        assert_eq!(db::problem_list(&conn, None).unwrap().len(), 0);

        // bad -> one problem, not an incident
        let c = check_and_open_problems(&conn, &[good.clone(), bad.clone()]).unwrap();
        assert_eq!(c, vec!["memory".to_string()]);
        assert_eq!(db::problem_list(&conn, None).unwrap().len(), 1);
        assert_eq!(db::incident_list(&conn, None).unwrap().len(), 0, "conflict is a problem, never an incident");

        // idempotent: re-checking does not open a second problem
        check_and_open_problems(&conn, &[bad]).unwrap();
        assert_eq!(db::problem_list(&conn, None).unwrap().len(), 1);
    }

    #[test]
    fn cadence_tune_is_read_from_to_be_state() {
        let sys = tmpdir();
        let user = tmpdir();
        fs::write(sys.join("disk.md"), "# d\n\n```assertions\ncadence = \"every 15m\"\n[[assert]]\nkey=\"a\"\nop=\"lt\"\nvalue=1\n```\n").unwrap();
        fs::write(sys.join("net.md"), "# n\n").unwrap();
        assert_eq!(cadence_tune(&sys, &user, "disk").as_deref(), Some("every 15m"));
        assert_eq!(cadence_tune(&sys, &user, "net"), None);
        assert_eq!(cadence_tune(&sys, &user, "absent"), None);
        fs::remove_dir_all(&sys).ok();
        fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn recurrence_threshold_override_then_default() {
        let sys = tmpdir();
        let user = tmpdir();
        // domain with an explicit per-domain override
        fs::write(sys.join("disk.md"), "# d\n\n```assertions\nrecurrence_threshold = 7\n[[assert]]\nkey=\"a\"\nop=\"lt\"\nvalue=1\n```\n").unwrap();
        // domain without an override
        fs::write(sys.join("net.md"), "# n\n").unwrap();

        assert_eq!(recurrence_threshold(&sys, &user, "disk", 3), 7, "override wins");
        assert_eq!(recurrence_threshold(&sys, &user, "net", 3), 3, "falls back to default");
        assert_eq!(recurrence_threshold(&sys, &user, "absent", 3), 3, "missing domain -> default");

        // a user override shadows the system file's threshold
        fs::write(user.join("disk.md"), "# d\n\n```assertions\nrecurrence_threshold = 2\n[[assert]]\nkey=\"a\"\nop=\"lt\"\nvalue=1\n```\n").unwrap();
        assert_eq!(recurrence_threshold(&sys, &user, "disk", 3), 2, "user layer wins");

        fs::remove_dir_all(&sys).ok();
        fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn summaries_flag_conflicts() {
        let good = parse(DISK_MD);
        let s = summaries(&[good]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].assertions, 2);
        assert_eq!(s[0].recurrence_threshold, Some(5));
        assert!(s[0].conflicts.is_empty());
    }
}
