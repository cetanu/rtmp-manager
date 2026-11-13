//! User-defined message patterns and scripts. Durable state and execution records are generic.
pub mod alerts;
pub mod runtime;
use crate::chat::IncomingChatMessage;
use crate::util::now_unix_ms;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use toasty::Executor;

/// Owns live execution and the shared alert schedule.
#[derive(Clone)]
pub struct AutomationEngine {
    db: toasty::Db,
    status: tokio::sync::watch::Receiver<crate::server::preview::StreamStatus>,
    queue: alerts::AlertQueue,
}

impl AutomationEngine {
    pub fn new(
        db: toasty::Db,
        status: tokio::sync::watch::Receiver<crate::server::preview::StreamStatus>,
    ) -> Self {
        Self {
            db: db.clone(),
            status,
            queue: alerts::AlertQueue::new(db),
        }
    }

    pub fn subscribe_alerts(&self) -> tokio::sync::broadcast::Receiver<alerts::Alert> {
        self.queue.subscribe()
    }

    pub async fn test_draft(
        &self,
        pattern: String,
        script: String,
        text: String,
    ) -> Result<Option<DraftResult>> {
        let result = tokio::task::spawn_blocking(move || {
            let definition = AutomationDefinition {
                name: "Test".into(),
                pattern,
                script,
                enabled: false,
                consume: false,
            };
            let message = IncomingChatMessage {
                source: crate::chat::Source::Twitch,
                external_id: "test-message".into(),
                author: "test_viewer".into(),
                user_id: Some("test-viewer".into()),
                text,
                parts: vec![],
                avatar_url: None,
                sent_at: None,
            };
            runtime::test_message(&definition, &message)
        })
        .await??;
        let Some(result) = result else {
            return Ok(None);
        };
        let preview = if result.alerts.is_empty() {
            None
        } else {
            Some(
                match alerts::validate_media(&self.db, &result.alerts).await {
                    Ok(()) => self.queue.enqueue(result.alerts.clone()).await,
                    Err(error) => Err(error),
                },
            )
        };
        Ok(Some(DraftResult { result, preview }))
    }

    pub async fn process_message(
        &self,
        message: &IncomingChatMessage,
    ) -> Result<Option<ProcessOutcome>> {
        if was_processed(&self.db, message).await? {
            return Ok(None);
        }
        if self.status.borrow().state != crate::server::preview::StreamState::Live {
            return Ok(Some(ProcessOutcome::empty()));
        }
        let mut outcome = process(&self.db, message).await?;
        if let Err(error) = self
            .queue
            .enqueue(std::mem::take(&mut outcome.alerts))
            .await
        {
            tracing::warn!("Could not queue committed automation alerts: {error:#}");
        }
        Ok(Some(outcome))
    }
}

pub struct DraftResult {
    pub result: runtime::ScriptResult,
    pub preview: Option<Result<usize>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationDefinition {
    pub name: String,
    pub pattern: String,
    pub script: String,
    pub enabled: bool,
    pub consume: bool,
}

#[derive(Debug, toasty::Model)]
#[table = "chat_automations"]
struct StoredAutomation {
    #[key]
    #[auto]
    id: u64,
    name: String,
    pattern: String,
    script: String,
    enabled: bool,
    consume: bool,
}

#[derive(Debug, toasty::Model)]
#[table = "chat_automation_data"]
#[unique(name = "index_automation_data_scope", automation_id, scope)]
struct StoredData {
    #[key]
    #[auto]
    id: u64,
    automation_id: u64,
    scope: String,
    data: String,
}

#[derive(Debug, toasty::Model)]
#[table = "chat_automation_runs"]
#[unique(
    name = "index_automation_run_message",
    automation_id,
    source,
    external_id
)]
#[index(name = "index_automation_message", source, external_id)]
struct StoredRun {
    #[key]
    #[auto]
    id: u64,
    automation_id: u64,
    automation_name: String,
    source: String,
    external_id: String,
    author: String,
    output: String,
    error: String,
    received_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    pub id: u64,
    pub definition: AutomationDefinition,
}
impl From<StoredAutomation> for Automation {
    fn from(row: StoredAutomation) -> Self {
        Self {
            id: row.id,
            definition: AutomationDefinition {
                name: row.name,
                pattern: row.pattern,
                script: row.script,
                enabled: row.enabled,
                consume: row.consume,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub automation_name: String,
    pub author: String,
    pub source: String,
    pub output: String,
    pub error: String,
    pub received_at_unix_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationSnapshot {
    pub automations: Vec<Automation>,
    pub recent: Vec<Run>,
}

pub async fn snapshot(db: &toasty::Db) -> Result<AutomationSnapshot> {
    let mut db = db.clone();
    let automations = StoredAutomation::all()
        .order_by(StoredAutomation::fields().id().asc())
        .exec(&mut db)
        .await?
        .into_iter()
        .map(Automation::from)
        .collect();
    let recent = StoredRun::all()
        .order_by(StoredRun::fields().id().desc())
        .limit(20)
        .exec(&mut db)
        .await?
        .into_iter()
        .map(|row| Run {
            automation_name: row.automation_name,
            author: row.author,
            source: row.source,
            output: row.output,
            error: row.error,
            received_at_unix_ms: row.received_at_unix_ms,
        })
        .collect();
    Ok(AutomationSnapshot {
        automations,
        recent,
    })
}

pub async fn save(
    db: &toasty::Db,
    id: Option<u64>,
    mut definition: AutomationDefinition,
) -> Result<()> {
    definition.name = definition.name.trim().to_string();
    // Validate on the blocking pool as scripts and regular expressions are compiled here.
    let validated = definition.clone();
    tokio::task::spawn_blocking(move || runtime::validate(&validated)).await??;
    let mut db = db.clone();
    if let Some(id) = id {
        let mut row = StoredAutomation::filter(StoredAutomation::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?
            .context("Automation no longer exists")?;
        row.update()
            .name(definition.name)
            .pattern(definition.pattern)
            .script(definition.script)
            .enabled(definition.enabled)
            .consume(definition.consume)
            .exec(&mut db)
            .await?;
    } else {
        anyhow::ensure!(
            StoredAutomation::all().count().exec(&mut db).await? < 64,
            "At most 64 automations are supported"
        );
        toasty::create!(StoredAutomation {
            name: definition.name,
            pattern: definition.pattern,
            script: definition.script,
            enabled: definition.enabled,
            consume: definition.consume
        })
        .exec(&mut db)
        .await?;
    }
    Ok(())
}

pub async fn set_enabled(db: &toasty::Db, id: u64, enabled: bool) -> Result<()> {
    let mut db = db.clone();
    let mut row = StoredAutomation::filter(StoredAutomation::fields().id().eq(id))
        .first()
        .exec(&mut db)
        .await?
        .context("Automation no longer exists")?;
    row.update().enabled(enabled).exec(&mut db).await?;
    Ok(())
}

pub async fn delete(db: &toasty::Db, id: u64) -> Result<()> {
    let mut db = db.clone();
    let mut tx = db.transaction().await?;
    let row = StoredAutomation::filter(StoredAutomation::fields().id().eq(id))
        .first()
        .exec(&mut tx)
        .await?
        .context("Automation no longer exists")?;
    row.delete().exec(&mut tx).await?;
    toasty::sql::statement("DELETE FROM chat_automation_data WHERE automation_id = ?1")
        .bind(id)
        .exec(&mut tx)
        .await?;
    // Keep executions as an audit trail and delivery deduplication ledger.
    tx.commit().await?;
    Ok(())
}

async fn load_data(
    db: &mut dyn Executor,
    automation_id: u64,
    scope: &str,
) -> Result<Map<String, Value>> {
    let row = StoredData::filter(
        StoredData::fields()
            .automation_id()
            .eq(automation_id)
            .and(StoredData::fields().scope().eq(scope)),
    )
    .first()
    .exec(db)
    .await?;
    Ok(match row {
        Some(row) => serde_json::from_str(&row.data)?,
        None => Map::new(),
    })
}

async fn save_data(
    db: &mut dyn Executor,
    automation_id: u64,
    scope: &str,
    data: Map<String, Value>,
) -> Result<()> {
    let row = StoredData::filter(
        StoredData::fields()
            .automation_id()
            .eq(automation_id)
            .and(StoredData::fields().scope().eq(scope)),
    )
    .first()
    .exec(db)
    .await?;
    if let Some(mut row) = row {
        row.update()
            .data(serde_json::to_string(&data)?)
            .exec(db)
            .await?;
    } else if !data.is_empty() {
        toasty::create!(StoredData {
            automation_id,
            scope: scope.to_owned(),
            data: serde_json::to_string(&data)?
        })
        .exec(db)
        .await?;
    }
    Ok(())
}

pub async fn was_processed(db: &toasty::Db, message: &IncomingChatMessage) -> Result<bool> {
    Ok(StoredRun::filter(
        StoredRun::fields()
            .source()
            .eq(message.source.as_str())
            .and(StoredRun::fields().external_id().eq(&message.external_id)),
    )
    .first()
    .exec(&mut db.clone())
    .await?
    .is_some())
}

pub struct ProcessOutcome {
    pub executed: bool,
    pub succeeded: u64,
    pub failed: u64,
    pub consumed: bool,
    pub alerts: Vec<alerts::Alert>,
}

impl ProcessOutcome {
    fn empty() -> Self {
        Self {
            executed: false,
            succeeded: 0,
            failed: 0,
            consumed: false,
            alerts: Vec::new(),
        }
    }
}

/// The actor calls this only while Live. All matching scripts run in ID order.
/// Successful state changes and all execution records commit together.
pub async fn process(db: &toasty::Db, message: &IncomingChatMessage) -> Result<ProcessOutcome> {
    let media_db = db.clone();
    let mut db = db.clone();
    let mut tx = db.transaction().await?;
    let mut outcome = ProcessOutcome {
        executed: false,
        succeeded: 0,
        failed: 0,
        consumed: false,
        alerts: Vec::new(),
    };
    let rules = StoredAutomation::filter(StoredAutomation::fields().enabled().eq(true))
        .order_by(StoredAutomation::fields().id().asc())
        .exec(&mut tx)
        .await?;
    for rule in rules {
        let regex = regex::Regex::new(&rule.pattern)?;
        let Some(captures) = regex.captures(&message.text) else {
            continue;
        };
        let captures = captures
            .iter()
            .map(|m| m.map(|m| m.as_str()).unwrap_or("").to_owned())
            .collect();
        let state = load_data(&mut tx, rule.id, "global").await?;
        let viewer_key = message
            .user_id
            .as_ref()
            .map(|id| format!("{}:{id}", message.source));
        let viewer = match &viewer_key {
            Some(key) => load_data(&mut tx, rule.id, key).await?,
            None => Map::new(),
        };
        let script = rule.script.clone();
        let incoming = message.clone();
        let now = now_unix_ms();
        let result = tokio::task::spawn_blocking(move || {
            runtime::evaluate(&script, &incoming, captures, state, viewer, now)
        })
        .await?;
        let result = match result {
            Ok(result) => alerts::validate_media(&media_db, &result.alerts)
                .await
                .map(|()| result),
            Err(error) => Err(error),
        };
        let (output, error) = match result {
            Ok(result) => {
                save_data(&mut tx, rule.id, "global", result.state).await?;
                if let Some(key) = viewer_key {
                    save_data(&mut tx, rule.id, &key, result.viewer).await?;
                }
                outcome.alerts.extend(result.alerts);
                outcome.consumed |= rule.consume;
                (result.output, String::new())
            }
            Err(error) => (
                String::new(),
                error.to_string().chars().take(4096).collect(),
            ),
        };
        if error.is_empty() {
            outcome.succeeded += 1;
        } else {
            outcome.failed += 1;
        }
        toasty::create!(StoredRun {
            automation_id: rule.id,
            automation_name: rule.name,
            source: message.source.to_string(),
            external_id: message.external_id.clone(),
            author: message.author.clone(),
            output,
            error,
            received_at_unix_ms: now,
        })
        .exec(&mut tx)
        .await?;
        outcome.executed = true;
    }
    tx.commit().await?;
    Ok(outcome)
}

#[cfg(test)]
mod tests;
