//! FEAT-046: validate the shipped `regin-operator-skills` package — every
//! operator-skill manifest parses, every domain has a consistent to-be-state, and
//! every remediation maps to a sane lane. Runs against the source assets that the
//! nfpm recipe (FEAT-053) packages into /usr/share/regin.

use std::path::PathBuf;

use regin_core::desired::{self, DesiredSource};
use regin_core::opskill;
use regin_core::remediation::{self, RiskClass};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("assets")
}

/// The broad v1 catalog (DISC-012): ~12 domains.
const EXPECTED_DOMAINS: &[&str] = &[
    "disk",
    "services",
    "memory-load",
    "logs",
    "security-updates",
    "certificates",
    "backups",
    "network",
    "time-sync",
    "users-auth",
    "processes",
    "firewall",
];

#[test]
fn catalog_has_the_broad_domain_set() {
    let skills = opskill::load_all(&assets().join("operator-skills"), &PathBuf::from("/nonexistent"));
    let mut domains: Vec<&str> = skills.iter().map(|s| s.domain.as_str()).collect();
    domains.sort_unstable();
    let mut expected: Vec<&str> = EXPECTED_DOMAINS.to_vec();
    expected.sort_unstable();
    assert_eq!(domains, expected, "operator-skills package must ship the broad v1 catalog");
}

#[test]
fn every_skill_parses_and_has_a_monitor() {
    let skills = opskill::load_all(&assets().join("operator-skills"), &PathBuf::from("/nonexistent"));
    assert_eq!(skills.len(), EXPECTED_DOMAINS.len(), "no manifest should fail to parse");
    for s in &skills {
        assert!(!s.monitor_command.trim().is_empty(), "{} has a monitor", s.domain);
        for r in &s.remediations {
            assert!(!r.command.trim().is_empty(), "{} remediation `{}` has a command", s.domain, r.title);
            // tags, when present, must be on the pre-blessed safe-action allowlist
            if let Some(tag) = &r.tag {
                assert!(remediation::is_preblessed(tag), "{}: tag `{tag}` must be pre-blessed", s.domain);
            }
        }
    }
}

#[test]
fn every_domain_has_a_consistent_to_be_state() {
    let skills = opskill::load_all(&assets().join("operator-skills"), &PathBuf::from("/nonexistent"));
    let desired_dir = assets().join("desired");
    for s in &skills {
        let ds = desired::load_desired(&desired_dir, &PathBuf::from("/nonexistent"), &s.domain)
            .unwrap()
            .unwrap_or_else(|| panic!("domain `{}` must ship a to-be-state file", s.domain));
        assert!(!ds.assertions.is_empty(), "{} declares at least one assertion", s.domain);
        assert!(
            desired::contradictions(&ds).is_empty(),
            "{} to-be-state must be internally consistent: {:?}",
            s.domain,
            desired::contradictions(&ds)
        );
    }
}

#[test]
fn remediating_domains_offer_safe_or_approval_fixes() {
    let skills = opskill::load_all(&assets().join("operator-skills"), &PathBuf::from("/nonexistent"));
    // Domains DISC-012 marked as remediating must carry a playbook.
    for dom in ["disk", "services", "logs", "time-sync", "backups", "security-updates"] {
        let s = skills.iter().find(|s| s.domain == dom).unwrap();
        assert!(!s.remediations.is_empty(), "{dom} ships a remediation playbook");
        // every remediation maps to a usable candidate fix
        for r in &s.remediations {
            let fix = r.to_candidate_fix();
            assert!(matches!(fix.risk, RiskClass::Safe | RiskClass::Uncertain | RiskClass::OutOfControl));
        }
    }
    // Monitor-only domains escalate (no playbook).
    for dom in ["memory-load", "certificates", "network", "users-auth", "processes", "firewall"] {
        let s = skills.iter().find(|s| s.domain == dom).unwrap();
        assert!(s.remediations.is_empty(), "{dom} is monitor-only (escalate, no auto-fix)");
    }
}

#[test]
fn desired_files_load_as_a_layered_set() {
    // The whole desired/ set loads (fail-safe) and every domain is consistent.
    let states = desired::load_all_desired(&assets().join("desired"), &PathBuf::from("/nonexistent"));
    assert_eq!(states.len(), EXPECTED_DOMAINS.len());
    assert_eq!(
        DesiredSource::System,
        states[0].source,
        "shipped files load as the system layer"
    );
}
