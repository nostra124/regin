pub mod approval;
pub mod audit;
pub mod bus;
pub mod chair;
pub mod config;
pub mod context;
pub mod decision;
pub mod deputy;
pub mod desired;
pub mod evaluate;
pub mod filters;
pub mod escalation;
pub mod foreman;
pub mod goal;
pub mod greeting;
pub mod guardrail;
pub mod db;
pub mod identity_db;
pub mod kpi;
pub mod llm;
pub mod mode;
pub mod objective;
pub mod opskill;
pub mod persona;
pub mod planning;
pub mod posture;
pub mod promotion;
pub mod protocol;
pub mod push;
pub mod reflect;
pub mod remediation;
pub mod resilience;
pub mod repo;
pub mod safelane;
pub mod schedule;
pub mod skillpkg;
pub mod skills;
pub mod soul;
pub mod tools;
pub mod two_tier;
pub mod types;
pub mod worker;

/// Test-only synchronization for tests that mutate XDG_* env vars (FEAT-075).
/// `config.rs` and `context.rs` both read `dirs::config_dir()`-derived paths;
/// `cargo test` runs a crate's tests concurrently on multiple threads within
/// one process, so any test that temporarily overrides e.g. `XDG_CONFIG_HOME`
/// must hold this lock for its whole duration — otherwise an unrelated,
/// concurrently-running test reading the same env var could observe the
/// override and flake.
#[cfg(test)]
pub(crate) mod xdg_env_lock {
    pub static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
