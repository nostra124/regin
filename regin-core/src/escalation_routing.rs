//! Source-routed escalation (FEAT-069 / DISC-019).
//!
//! Wires the real DISC-010 channel behind `control_loop::EscalationSink`
//! (FEAT-066 left it injectable specifically so this ticket could plug in
//! the real routing without touching the control loop itself). Routed by
//! the escalated intent's own `source` (`objective::IntentSource`):
//! - **dvalin** — sent over the bus (FEAT-010), the dvalin-hierarchy
//!   supervisor channel.
//! - **human** — a critical push is attempted first (FEAT-044); a failure
//!   there is never fatal (push.rs's own "the item is already parked" — a
//!   push failure just means it waits for the pull channel) — the
//!   escalation is *always* also parked for the next login greeting
//!   (FEAT-043), so it's never lost either way.
//! - **regin** — parked only; regin has no external channel of its own to
//!   escalate a self-authored intent over.
//!
//! **Parking reuses the existing generic episodic-memory table** (`kind =
//! "intent_escalation"`, `db::episode_record`/the `episodes` table) rather
//! than a new schema — the same store `greeting::build` already reads
//! `pending_changes`/`decision_problems` from.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};

use crate::control_loop::{EscalationSink, PlanningEscalation};
use crate::db;
use crate::objective::IntentSource;

const ESCALATION_EPISODE_KIND: &str = "intent_escalation";

/// Sends a structured escalation to dvalin over the bus (FEAT-010).
/// Injectable — the real implementation wraps `bus::BusClient::send`;
/// tests use a spy.
#[async_trait]
pub trait DvalinBusSink: Send + Sync {
    async fn send_escalation(&self, escalation: &PlanningEscalation) -> Result<()>;
}

/// Attempts a critical push (FEAT-044). Injectable; a failure here is
/// never fatal to the escalation — the caller always parks regardless.
#[async_trait]
pub trait CriticalPushSink: Send + Sync {
    async fn push_escalation(&self, escalation: &PlanningEscalation) -> Result<()>;
}

/// A `CriticalPushSink` for a deployment with no push channel configured —
/// push.rs's own default-off posture (FEAT-044: "opt-in, off-by-default").
pub struct NoPush;

#[async_trait]
impl CriticalPushSink for NoPush {
    async fn push_escalation(&self, _escalation: &PlanningEscalation) -> Result<()> {
        Err(anyhow!("no push channel configured"))
    }
}

/// Persist an escalation into the episodic store so the login greeting can
/// surface it.
pub fn park_escalation(conn: &Connection, escalation: &PlanningEscalation) -> Result<()> {
    let detail = serde_json::to_string(escalation)?;
    db::episode_record(conn, ESCALATION_EPISODE_KIND, Some(&escalation.goal_id), &escalation.reason, Some(&detail))?;
    Ok(())
}

/// Escalations parked and not yet reflected away — what the greeting
/// surfaces (acceptance criterion 3).
pub fn pending_escalations(conn: &Connection) -> Result<Vec<PlanningEscalation>> {
    let mut stmt = conn.prepare(
        "SELECT detail FROM episodes WHERE kind = ?1 AND reflected = 0 AND detail IS NOT NULL ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map(params![ESCALATION_EPISODE_KIND], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().filter_map(|d| serde_json::from_str(&d).ok()).collect())
}

/// Routes an escalation to its intent's source over the correct DISC-010
/// channel (acceptance criterion 2). Holds `Arc<Mutex<Connection>>` (not a
/// bare reference) so it satisfies `EscalationSink: Send + Sync` — the same
/// shape `decision::IdentityDbSink` already uses, since `rusqlite::
/// Connection` isn't `Sync` on its own.
pub struct SourceRoutedEscalationSink {
    conn: Arc<Mutex<Connection>>,
    bus: Arc<dyn DvalinBusSink>,
    push: Arc<dyn CriticalPushSink>,
}

impl SourceRoutedEscalationSink {
    pub fn new(conn: Arc<Mutex<Connection>>, bus: Arc<dyn DvalinBusSink>, push: Arc<dyn CriticalPushSink>) -> Self {
        Self { conn, bus, push }
    }
}

#[async_trait]
impl EscalationSink for SourceRoutedEscalationSink {
    async fn escalate(&self, escalation: &PlanningEscalation) -> Result<()> {
        match IntentSource::parse(&escalation.source)? {
            IntentSource::Dvalin => {
                self.bus.send_escalation(escalation).await?;
            }
            IntentSource::Human => {
                if let Err(e) = self.push.push_escalation(escalation).await {
                    tracing::warn!(goal_id = %escalation.goal_id, error = %e, "critical push failed; parked for the login greeting");
                }
                let conn = self.conn.lock().map_err(|_| anyhow!("regin.db mutex poisoned"))?;
                park_escalation(&conn, escalation)?;
            }
            IntentSource::Regin => {
                let conn = self.conn.lock().map_err(|_| anyhow!("regin.db mutex poisoned"))?;
                park_escalation(&conn, escalation)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_loop::standard_remedies;
    use crate::db as regin_db;
    use std::sync::Mutex as StdMutex;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        regin_db::init_schema(&c).unwrap();
        c
    }

    fn an_escalation(source: &str) -> PlanningEscalation {
        PlanningEscalation {
            goal_id: "goal-1".into(),
            source: source.into(),
            reason: "tasks still failing after mitigate/replan: t1".into(),
            remedies: standard_remedies(),
        }
    }

    struct SpyBus {
        calls: StdMutex<Vec<PlanningEscalation>>,
    }
    impl SpyBus {
        fn new() -> Self {
            Self { calls: StdMutex::new(vec![]) }
        }
    }
    #[async_trait]
    impl DvalinBusSink for SpyBus {
        async fn send_escalation(&self, escalation: &PlanningEscalation) -> Result<()> {
            self.calls.lock().unwrap().push(escalation.clone());
            Ok(())
        }
    }

    struct SpyPush {
        calls: StdMutex<Vec<PlanningEscalation>>,
    }
    impl SpyPush {
        fn new() -> Self {
            Self { calls: StdMutex::new(vec![]) }
        }
    }
    #[async_trait]
    impl CriticalPushSink for SpyPush {
        async fn push_escalation(&self, escalation: &PlanningEscalation) -> Result<()> {
            self.calls.lock().unwrap().push(escalation.clone());
            Ok(())
        }
    }

    struct FailingPush;
    #[async_trait]
    impl CriticalPushSink for FailingPush {
        async fn push_escalation(&self, _escalation: &PlanningEscalation) -> Result<()> {
            Err(anyhow!("no network"))
        }
    }

    #[tokio::test]
    async fn a_dvalin_sourced_escalation_goes_over_the_bus_only() {
        // acceptance criterion 2
        let c = Arc::new(Mutex::new(conn()));
        let bus = Arc::new(SpyBus::new());
        let push = Arc::new(SpyPush::new());
        let sink = SourceRoutedEscalationSink::new(c.clone(), bus.clone(), push.clone());

        sink.escalate(&an_escalation("dvalin")).await.unwrap();

        assert_eq!(bus.calls.lock().unwrap().len(), 1);
        assert!(push.calls.lock().unwrap().is_empty());
        let conn = c.lock().unwrap();
        assert!(pending_escalations(&conn).unwrap().is_empty(), "dvalin's own channel doesn't also park");
    }

    #[tokio::test]
    async fn a_human_sourced_escalation_pushes_and_always_parks() {
        // acceptance criterion 2
        let c = Arc::new(Mutex::new(conn()));
        let bus = Arc::new(SpyBus::new());
        let push = Arc::new(SpyPush::new());
        let sink = SourceRoutedEscalationSink::new(c.clone(), bus.clone(), push.clone());

        sink.escalate(&an_escalation("human")).await.unwrap();

        assert!(bus.calls.lock().unwrap().is_empty());
        assert_eq!(push.calls.lock().unwrap().len(), 1);
        let conn = c.lock().unwrap();
        let parked = pending_escalations(&conn).unwrap();
        assert_eq!(parked.len(), 1);
        assert_eq!(parked[0].goal_id, "goal-1");
        assert_eq!(parked[0].remedies, standard_remedies());
    }

    #[tokio::test]
    async fn a_failed_push_never_blocks_parking_for_a_human_source() {
        let c = Arc::new(Mutex::new(conn()));
        let bus = Arc::new(SpyBus::new());
        let sink = SourceRoutedEscalationSink::new(c.clone(), bus.clone(), Arc::new(FailingPush));

        sink.escalate(&an_escalation("human")).await.unwrap();

        let conn = c.lock().unwrap();
        assert_eq!(pending_escalations(&conn).unwrap().len(), 1, "still parked despite the push failure");
    }

    #[tokio::test]
    async fn a_regin_sourced_escalation_only_parks_no_bus_or_push() {
        let c = Arc::new(Mutex::new(conn()));
        let bus = Arc::new(SpyBus::new());
        let push = Arc::new(SpyPush::new());
        let sink = SourceRoutedEscalationSink::new(c.clone(), bus.clone(), push.clone());

        sink.escalate(&an_escalation("regin")).await.unwrap();

        assert!(bus.calls.lock().unwrap().is_empty());
        assert!(push.calls.lock().unwrap().is_empty());
        let conn = c.lock().unwrap();
        assert_eq!(pending_escalations(&conn).unwrap().len(), 1);
    }

    #[test]
    fn pending_escalations_round_trips_the_full_payload() {
        let c = conn();
        park_escalation(&c, &an_escalation("regin")).unwrap();
        let parked = pending_escalations(&c).unwrap();
        assert_eq!(parked, vec![an_escalation("regin")]);
    }

    #[test]
    fn park_escalation_never_touches_unrelated_episode_kinds() {
        let c = conn();
        db::episode_record(&c, "change", None, "unrelated", None).unwrap();
        park_escalation(&c, &an_escalation("regin")).unwrap();
        assert_eq!(pending_escalations(&c).unwrap().len(), 1);
    }
}
