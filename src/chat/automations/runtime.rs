//! Bounded synchronous script evaluation, with no filesystem, networking or database access.
use super::{AutomationDefinition, alerts::Alert};
use crate::chat::IncomingChatMessage;
use anyhow::{Result, anyhow};
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use parking_lot::Mutex;
use rhai::{Dynamic, Engine, EvalAltResult, Scope};
use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Clone)]
struct Overlay(Arc<Mutex<Vec<Alert>>>);

pub const EXAMPLE_SCRIPT: &str = r#"// This script runs when the message matches your pattern.
// Example pattern: (?i)^!roll(?:\s+(\w+))?$
// captures[0] is the full match; later entries are capture groups.
let matched = captures[0];
let argument = if captures.len() > 1 { captures[1] } else { "" };

// Message fields: platform, id, author, user_id and text.
// user_id is empty when the platform did not provide an account ID.
let author = message.author;
let platform = message.platform;
let message_id = message.id;
let text = message.text;

// state is shared by this automation and persists between executions.
let total = if state.contains("total") { state.total } else { 0 };
state.total = total + 1;
state.last_run_ms = now_ms; // Current Unix time in milliseconds.

// viewer persists separately for each platform account. Only write it
// when a user ID is available.
let visits = 0;
if message.user_id != "" {
    visits = if viewer.contains("visits") { viewer.visits } else { 0 };
    visits += 1;
    viewer.visits = visits;
}

let roll = random_int(100); // A random integer from 1 through 100.
let result = author + " rolled " + roll.to_string();

// Alerts appear on the OBS alert overlay. At least one of text, image or
// sound is required; duration_ms must be between 250 and 60000.
overlay.show(#{
    text: result,
    // image: "your-uploaded-image.gif", // Uncomment after uploading it.
    // sound: "your-uploaded-sound.mp3", // Uncomment after uploading it.
    duration_ms: 8000
});

// The final string appears in Recent executions (it is not sent to chat).
// Return () instead to leave the output empty.
result
"#;

pub const GIVEAWAY_SCRIPT: &str = r#"// One attempt per platform account. Change these rules however you like.
if message.user_id == "" { throw "A platform user ID is required"; }
let attempts = if viewer.contains("attempts") { viewer.attempts } else { 0 };
if attempts > 0 { return message.author + ": already rolled"; }
viewer.attempts = attempts + 1;
let rolled = random_int(10000); // Uniform integer from 1 through 10,000.
viewer.last_roll = rolled;
if rolled == 1 {
    viewer.winner = true;
    message.author + " WON!"
} else {
    message.author + " rolled " + rolled.to_string()
}"#;

pub const POINTS_SCRIPT: &str = r#"if message.user_id == "" { throw "A platform user ID is required"; }
let points = if viewer.contains("points") { viewer.points } else { 0 };
viewer.points = points + 1;
message.author + " has " + viewer.points.to_string() + " points"
"#;

fn engine() -> Engine {
    let mut engine = Engine::new();
    engine
        .set_max_operations(20_000)
        .set_max_call_levels(16)
        .set_max_expr_depths(32, 16)
        .set_max_variables(64)
        .set_max_functions(16)
        .set_max_string_size(8192)
        .set_max_array_size(256)
        .set_max_map_size(256);
    engine.register_type_with_name::<Overlay>("Overlay");
    engine.register_fn(
        "show",
        |overlay: &mut Overlay, options: rhai::Map| -> Result<(), Box<EvalAltResult>> {
            let alert: Alert = rhai::serde::from_dynamic(&Dynamic::from_map(options))
                .map_err(|e| Box::<EvalAltResult>::from(e.to_string()))?;
            alert
                .validate()
                .map_err(|e| Box::<EvalAltResult>::from(e.to_string()))?;
            let mut alerts = overlay.0.lock();
            if alerts.len() >= 8 {
                return Err("At most 8 alerts per execution".into());
            }
            alerts.push(alert);
            Ok(())
        },
    );
    engine.on_print(|_| {});
    engine.on_debug(|_, _, _| {});
    engine.register_fn(
        "random_int",
        |upper: i64| -> Result<i64, Box<EvalAltResult>> {
            if upper < 1 {
                return Err("random_int requires a positive upper bound".into());
            }
            let sides = upper as u64;
            let threshold = sides.wrapping_neg() % sides;
            loop {
                let mut bytes = [0; 8];
                SystemRandom::new()
                    .fill(&mut bytes)
                    .map_err(|_| Box::<EvalAltResult>::from("Random generator failed"))?;
                let value = u64::from_le_bytes(bytes);
                if value >= threshold {
                    return Ok((value % sides + 1) as i64);
                }
            }
        },
    );
    engine
}

pub fn validate(definition: &AutomationDefinition) -> Result<()> {
    anyhow::ensure!(
        !definition.name.trim().is_empty() && definition.name.chars().count() <= 80,
        "Name must be 1–80 characters"
    );
    anyhow::ensure!(
        !definition.pattern.is_empty() && definition.pattern.len() <= 1024,
        "Pattern must be 1–1,024 bytes"
    );
    regex::Regex::new(&definition.pattern).map_err(|e| anyhow!("Invalid pattern: {e}"))?;
    anyhow::ensure!(
        !definition.script.trim().is_empty() && definition.script.len() <= 16_384,
        "Script must be 1–16,384 bytes"
    );
    engine()
        .compile(&definition.script)
        .map_err(|e| anyhow!("Invalid script: {e}"))?;
    Ok(())
}

pub struct ScriptResult {
    pub state: Map<String, Value>,
    pub viewer: Map<String, Value>,
    pub output: String,
    pub alerts: Vec<Alert>,
}

/// Runs an editor draft with fresh state and no persistent side effects.
pub fn test_message(
    definition: &AutomationDefinition,
    message: &IncomingChatMessage,
) -> Result<Option<ScriptResult>> {
    validate(definition)?;
    anyhow::ensure!(
        !message.text.is_empty() && message.text.len() <= 5000,
        "Test message must be 1–5,000 bytes"
    );
    let regex = regex::Regex::new(&definition.pattern)?;
    let Some(captures) = regex.captures(&message.text) else {
        return Ok(None);
    };
    let captures = captures
        .iter()
        .map(|m| m.map(|m| m.as_str()).unwrap_or("").to_owned())
        .collect();
    evaluate(
        &definition.script,
        message,
        captures,
        Map::new(),
        Map::new(),
        crate::util::now_unix_ms(),
    )
    .map(Some)
}

fn dynamic<T: serde::Serialize>(value: T) -> Result<Dynamic> {
    rhai::serde::to_dynamic(value).map_err(|e| anyhow!(e.to_string()))
}

fn read_map(scope: &Scope<'_>, name: &str) -> Result<Map<String, Value>> {
    let value = scope
        .get_value::<Dynamic>(name)
        .ok_or_else(|| anyhow!("Missing {name} data"))?;
    let map: Map<String, Value> = rhai::serde::from_dynamic(&value)
        .map_err(|e| anyhow!("{name} must remain a JSON object: {e}"))?;
    anyhow::ensure!(
        serde_json::to_vec(&map)?.len() <= 65_536,
        "{name} data exceeds 64 KiB"
    );
    Ok(map)
}

pub fn evaluate(
    script: &str,
    message: &IncomingChatMessage,
    captures: Vec<String>,
    state: Map<String, Value>,
    viewer: Map<String, Value>,
    now_ms: u64,
) -> Result<ScriptResult> {
    let engine = engine();
    let mut scope = Scope::new();
    let overlay = Overlay(Arc::new(Mutex::new(Vec::new())));
    scope.push("overlay", overlay.clone());
    scope.push_constant_dynamic("message", dynamic(serde_json::json!({
        "platform": message.source.as_str(), "id": message.external_id,
        "author": message.author, "user_id": message.user_id.as_deref().unwrap_or(""), "text": message.text,
    }))?);
    scope.push_constant_dynamic("captures", dynamic(captures)?);
    scope.push_constant("now_ms", i64::try_from(now_ms)?);
    scope.push_dynamic("state", dynamic(state)?);
    scope.push_dynamic("viewer", dynamic(viewer)?);
    let result: Dynamic = engine
        .eval_with_scope(&mut scope, script)
        .map_err(|e| anyhow!(e.to_string()))?;
    let output = if result.is_unit() {
        String::new()
    } else {
        result
            .try_cast::<String>()
            .ok_or_else(|| anyhow!("Script must return a string or ()"))?
    };
    anyhow::ensure!(output.len() <= 4096, "Script output exceeds 4 KiB");
    let state = read_map(&scope, "state")?;
    let viewer = read_map(&scope, "viewer")?;
    anyhow::ensure!(
        message.user_id.is_some() || viewer.is_empty(),
        "Viewer storage requires a platform user ID"
    );
    Ok(ScriptResult {
        state,
        viewer,
        output,
        alerts: overlay.0.lock().clone(),
    })
}
