use super::*;
use crate::{
    chat::{ChatHandle, EnqueueOutcome, Source},
    metrics::Metrics,
    server::preview::{StreamState, StreamStatus},
};
use reqwest::Client;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::watch;

static SEQUENCE: AtomicU64 = AtomicU64::new(1);
async fn database() -> (toasty::Db, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "rtmp-automations-{}-{}-{}.sqlite3",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = crate::db::connect(&path).await.unwrap();
    crate::db::run_migrations(&db).await.unwrap();
    (db, path)
}
fn message(id: &str) -> IncomingChatMessage {
    IncomingChatMessage {
        source: Source::Twitch,
        external_id: id.into(),
        author: "Viewer".into(),
        user_id: Some("123".into()),
        text: "!point".into(),
        parts: vec![],
        avatar_url: None,
        sent_at: None,
    }
}
fn definition() -> AutomationDefinition {
    AutomationDefinition {
        name: "Points".into(),
        pattern: "(?i)^!point$".into(),
        script: runtime::POINTS_SCRIPT.into(),
        enabled: true,
        consume: true,
    }
}

#[test]
fn example_script_runs_with_and_without_a_viewer_id() {
    let mut rule = definition();
    rule.pattern = r"(?i)^!point(?:\s+(\w+))?$".into();
    rule.script = runtime::EXAMPLE_SCRIPT.into();
    let mut incoming = message("example");
    let result = runtime::test_message(&rule, &incoming).unwrap().unwrap();
    assert_eq!(result.state["total"], 1);
    assert_eq!(result.viewer["visits"], 1);
    assert_eq!(result.alerts.len(), 1);
    assert!(result.output.starts_with("Viewer rolled "));

    incoming.user_id = None;
    let result = runtime::test_message(&rule, &incoming).unwrap().unwrap();
    assert!(result.viewer.is_empty());
    assert_eq!(result.alerts.len(), 1);
}
async fn data(db: &toasty::Db, id: u64, scope: &str) -> Map<String, Value> {
    load_data(&mut db.clone(), id, scope).await.unwrap()
}
async fn handle(db: &toasty::Db, status: watch::Receiver<StreamStatus>) -> ChatHandle {
    ChatHandle::spawn_with_database(
        db.clone(),
        1,
        15,
        Client::new(),
        Arc::new(Metrics::default()),
        status,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn repeated_text_is_filtered_before_automation() {
    let (db, _) = database().await;
    let (_status, rx) = watch::channel(StreamStatus {
        state: StreamState::Live,
    });
    let handle = handle(&db, rx).await;
    handle.save_automation(None, definition()).await.unwrap();
    assert_eq!(
        handle.enqueue(message("one")).await.unwrap(),
        EnqueueOutcome::Accepted
    );
    let mut repeated = message("two");
    repeated.text = "  !POINT  ".into();
    assert_eq!(
        handle.enqueue(repeated).await.unwrap(),
        EnqueueOutcome::Duplicate
    );
    let mut other = message("three");
    other.user_id = Some("other".into());
    assert_eq!(
        handle.enqueue(other).await.unwrap(),
        EnqueueOutcome::Accepted
    );
    assert_eq!(handle.automations().await.unwrap().recent.len(), 2);
}

#[tokio::test]
async fn scripts_are_gated_by_live_state_and_enabled_flag() {
    let (db, _) = database().await;
    let (status, rx) = watch::channel(StreamStatus {
        state: StreamState::Offline,
    });
    let handle = handle(&db, rx).await;
    let mut rule = definition();
    rule.pattern = "(?i)^!point".into();
    handle.save_automation(None, rule).await.unwrap();
    let distinct = |id: &str| {
        let mut incoming = message(id);
        incoming.text = format!("!point {id}");
        incoming
    };
    let id = handle.automations().await.unwrap().automations[0].id;
    handle.enqueue(distinct("offline")).await.unwrap();
    assert!(handle.automations().await.unwrap().recent.is_empty());
    status.send_replace(StreamStatus {
        state: StreamState::PreviewReady,
    });
    handle.enqueue(distinct("preview")).await.unwrap();
    assert!(handle.automations().await.unwrap().recent.is_empty());
    status.send_replace(StreamStatus {
        state: StreamState::Live,
    });
    // A message already displayed while offline must not be executed on redelivery.
    assert_eq!(
        handle.enqueue(distinct("offline")).await.unwrap(),
        EnqueueOutcome::Duplicate
    );
    handle.clear_messages().await.unwrap();
    handle.enqueue(distinct("live")).await.unwrap();
    assert_eq!(handle.snapshot().await.unwrap().queued, 0);
    assert_eq!(data(&db, id, "twitch:123").await["points"], 1);
    handle.toggle_automation(id, false).await.unwrap();
    handle.enqueue(distinct("disabled")).await.unwrap();
    assert_eq!(data(&db, id, "twitch:123").await["points"], 1);
    handle.toggle_automation(id, true).await.unwrap();
    status.send_replace(StreamStatus {
        state: StreamState::Offline,
    });
    handle.enqueue(distinct("stopped")).await.unwrap();
    assert_eq!(handle.automations().await.unwrap().recent.len(), 1);
}

#[tokio::test]
async fn generic_data_is_isolated_per_automation_and_platform_account_and_survives_reopen() {
    let (db, path) = database().await;
    save(&db, None, definition()).await.unwrap();
    let id = snapshot(&db).await.unwrap().automations[0].id;
    process(&db, &message("first")).await.unwrap();
    let mut renamed = message("renamed");
    renamed.author = "New name".into();
    process(&db, &renamed).await.unwrap();
    let mut other = message("youtube");
    other.source = Source::YouTube;
    process(&db, &other).await.unwrap();
    assert_eq!(data(&db, id, "twitch:123").await["points"], 2);
    assert_eq!(data(&db, id, "youtube:123").await["points"], 1);
    save(&db, None, definition()).await.unwrap();
    let second = snapshot(&db).await.unwrap().automations[1].id;
    assert!(data(&db, second, "twitch:123").await.is_empty());
    let reopened = crate::db::connect(&path).await.unwrap();
    assert_eq!(data(&reopened, id, "twitch:123").await["points"], 2);
    let mut updated = definition();
    updated.script = "viewer.points += 10; ()".into();
    save(&db, Some(id), updated).await.unwrap();
    assert_eq!(data(&db, id, "twitch:123").await["points"], 2);
    delete(&db, id).await.unwrap();
    assert!(data(&db, id, "twitch:123").await.is_empty());
    assert!(was_processed(&db, &message("first")).await.unwrap());
}

#[tokio::test]
async fn captures_and_shared_state_work_without_giveaway_specific_code() {
    let (db, _) = database().await;
    let rule = AutomationDefinition { name: "Counter".into(), pattern: r"^!add (\d+)$".into(),
        script: "state.total = if state.contains(\"total\") { state.total } else { 0 }; state.total += parse_int(captures[1]); state.total.to_string()".into(),
        enabled: true, consume: false };
    save(&db, None, rule).await.unwrap();
    let mut incoming = message("add");
    incoming.text = "!add 7".into();
    incoming.user_id = None;
    let outcome = process(&db, &incoming).await.unwrap();
    assert!(outcome.executed);
    assert_eq!((outcome.succeeded, outcome.failed), (1, 0));
    assert!(!outcome.consumed);
    assert_eq!(snapshot(&db).await.unwrap().recent[0].output, "7");
}

#[tokio::test]
async fn script_errors_discard_partial_state_and_do_not_stop_other_rules() {
    let (db, _) = database().await;
    let mut bad = definition();
    bad.name = "Broken".into();
    bad.script = "state.counter = 99; viewer.points = 500; throw \"broken script\";".into();
    save(&db, None, bad).await.unwrap();
    save(&db, None, definition()).await.unwrap();
    let ids: Vec<_> = snapshot(&db)
        .await
        .unwrap()
        .automations
        .iter()
        .map(|a| a.id)
        .collect();
    let outcome = process(&db, &message("errors")).await.unwrap();
    assert!(outcome.consumed);
    assert_eq!((outcome.succeeded, outcome.failed), (1, 1));
    assert!(data(&db, ids[0], "global").await.is_empty());
    assert!(data(&db, ids[0], "twitch:123").await.is_empty());
    assert_eq!(data(&db, ids[1], "twitch:123").await["points"], 1);
    assert!(
        snapshot(&db)
            .await
            .unwrap()
            .recent
            .iter()
            .any(|r| r.error.contains("broken script"))
    );
}

#[tokio::test]
async fn database_failure_rolls_back_state_and_execution_ledger() {
    let (db, _) = database().await;
    save(&db, None, definition()).await.unwrap();
    let id = snapshot(&db).await.unwrap().automations[0].id;
    let mut conn = db.clone();
    toasty::sql::statement("CREATE TRIGGER fail_execution BEFORE INSERT ON chat_automation_runs BEGIN SELECT RAISE(ABORT, 'test failure'); END").exec(&mut conn).await.unwrap();
    assert!(process(&db, &message("retry")).await.is_err());
    assert!(data(&db, id, "twitch:123").await.is_empty());
    assert!(!was_processed(&db, &message("retry")).await.unwrap());
    toasty::sql::statement("DROP TRIGGER fail_execution")
        .exec(&mut conn)
        .await
        .unwrap();
    process(&db, &message("retry")).await.unwrap();
    assert_eq!(data(&db, id, "twitch:123").await["points"], 1);
}

#[tokio::test]
async fn duplicate_messages_do_not_execute_after_seen_eviction_or_restart() {
    let (db, _) = database().await;
    let (_, rx) = watch::channel(StreamStatus {
        state: StreamState::Live,
    });
    let chat = handle(&db, rx.clone()).await;
    chat.save_automation(None, definition()).await.unwrap();
    chat.enqueue(message("original")).await.unwrap();
    for index in 0..10 {
        let mut incoming = message(&format!("ordinary-{index}"));
        incoming.text = "hello".into();
        chat.enqueue(incoming).await.unwrap();
    }
    let restarted = handle(&db, rx).await;
    assert_eq!(
        restarted.enqueue(message("original")).await.unwrap(),
        EnqueueOutcome::Duplicate
    );
    assert_eq!(restarted.automations().await.unwrap().recent.len(), 1);
}

#[test]
fn runtime_bounds_execution_and_rejects_invalid_definitions() {
    let mut rule = definition();
    rule.pattern = "[".into();
    assert!(runtime::validate(&rule).is_err());
    rule = definition();
    rule.script = "if {".into();
    assert!(runtime::validate(&rule).is_err());
    let error = runtime::evaluate(
        "loop {}",
        &message("loop"),
        vec![],
        Map::new(),
        Map::new(),
        1,
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("operations"), "{error}");
    assert!(
        runtime::evaluate(
            "import \"/etc/passwd\" as x;",
            &message("import"),
            vec![],
            Map::new(),
            Map::new(),
            1
        )
        .is_err()
    );
    let mut incoming = message("anonymous");
    incoming.user_id = None;
    assert!(
        runtime::evaluate(
            "viewer.points = 1; ()",
            &incoming,
            vec![],
            Map::new(),
            Map::new(),
            1
        )
        .is_err()
    );
}

#[test]
fn giveaway_and_points_are_ordinary_scripts() {
    for script in [runtime::GIVEAWAY_SCRIPT, runtime::POINTS_SCRIPT] {
        let result = runtime::evaluate(
            script,
            &message("examples"),
            vec![],
            Map::new(),
            Map::new(),
            1,
        )
        .unwrap();
        assert!(!result.output.is_empty());
        assert!(!result.viewer.is_empty());
    }
}

#[test]
fn draft_test_runs_with_captures_and_fresh_state() {
    let mut draft = definition();
    draft.pattern = "^!(point)$".into();
    draft.script =
        r#"state.command = captures[1]; viewer.points = if viewer.contains("points") { viewer.points + 1 } else { 1 }; message.author + ": " + message.text"#
            .into();
    let incoming = message("test");
    for _ in 0..2 {
        let result = runtime::test_message(&draft, &incoming).unwrap().unwrap();
        assert_eq!(result.output, "Viewer: !point");
        assert_eq!(result.state["command"], "point");
        assert_eq!(result.viewer["points"], 1);
    }
}

#[test]
fn draft_test_reports_mismatch_and_runtime_errors() {
    let mut draft = definition();
    draft.script = "asqfwf44fq".into();
    let mut incoming = message("test");
    incoming.text = "unmatched".into();
    assert!(runtime::test_message(&draft, &incoming).unwrap().is_none());
    incoming.text = "!point".into();
    let error = runtime::test_message(&draft, &incoming).err().unwrap();
    assert!(error.to_string().contains("asqfwf44fq"));
    draft.script = "let =".into();
    assert!(runtime::test_message(&draft, &incoming).is_err());
}

#[test]
fn overlay_actions_are_bounded_and_do_not_replace_text_output() {
    let script = r#"
        overlay.show(#{text: message.author + " WON!", duration_ms: 8000});
        viewer.winner = true;
        "Winner recorded"
    "#;
    let result =
        runtime::evaluate(script, &message("alert"), vec![], Map::new(), Map::new(), 1).unwrap();
    assert_eq!(result.output, "Winner recorded");
    assert_eq!(result.viewer["winner"], true);
    assert_eq!(result.alerts.len(), 1);
    assert_eq!(result.alerts[0].text, "Viewer WON!");
    let positioned = runtime::evaluate(
        "overlay.show(#{image: \"winner.gif\", position: #{x: 70, y: 10, width: 25, height: 30}, duration_ms: 1000});",
        &message("positioned"), vec![], Map::new(), Map::new(), 1,
    ).unwrap();
    assert_eq!(positioned.alerts[0].position.as_ref().unwrap().x, 70);
    for invalid in [
        "overlay.show(#{text: \"x\", duration_ms: 0});",
        "overlay.show(#{duration_ms: 1000});",
        "overlay.show(#{image: \"../secret\", duration_ms: 1000});",
        "overlay.show(#{text: \"x\", duration_ms: 1000, typo: true});",
        "overlay.show(#{text: \"x\", position: #{x: 80, y: 0, width: 30, height: 10}, duration_ms: 1000});",
        "overlay.show(#{text: \"x\", position: #{x: 0, y: 0, width: 0, height: 10}, duration_ms: 1000});",
        "for i in 0..9 { overlay.show(#{text: \"x\", duration_ms: 1000}); }",
    ] {
        assert!(
            runtime::evaluate(
                invalid,
                &message("invalid"),
                vec![],
                Map::new(),
                Map::new(),
                1
            )
            .is_err(),
            "{invalid}"
        );
    }
}

#[tokio::test]
async fn alerts_publish_after_commit_and_failed_scripts_publish_nothing() {
    let (db, _) = database().await;
    let (_status, rx) = watch::channel(StreamStatus {
        state: StreamState::Live,
    });
    let handle = handle(&db, rx).await;
    let mut rule = definition();
    rule.script =
        r#"viewer.winner = true; overlay.show(#{text: "Winner!", duration_ms: 1000}); "won""#
            .into();
    handle.save_automation(None, rule.clone()).await.unwrap();
    let id = handle.automations().await.unwrap().automations[0].id;
    let mut alerts = handle.subscribe_alerts();
    handle.enqueue(message("won")).await.unwrap();
    assert_eq!(alerts.try_recv().unwrap().text, "Winner!");
    assert_eq!(data(&db, id, "twitch:123").await["winner"], true);
    assert!(
        handle.subscribe_alerts().try_recv().is_err(),
        "New subscribers must not replay old alerts"
    );
    handle.enqueue(message("won")).await.unwrap();
    assert!(
        alerts.try_recv().is_err(),
        "Duplicate messages must not emit alerts"
    );
    rule.script = r#"viewer.failed = true; overlay.show(#{text: "Never play", duration_ms: 1000}); throw "failed";"#.into();
    handle
        .save_automation(Some(id), rule.clone())
        .await
        .unwrap();
    handle.enqueue(message("failed")).await.unwrap();
    assert!(alerts.try_recv().is_err());
    assert!(!data(&db, id, "twitch:123").await.contains_key("failed"));
    rule.script =
        r#"viewer.failed = true; overlay.show(#{image: "missing.png", duration_ms: 1000});"#.into();
    handle.save_automation(Some(id), rule).await.unwrap();
    handle.enqueue(message("missing")).await.unwrap();
    assert!(alerts.try_recv().is_err());
    assert!(!data(&db, id, "twitch:123").await.contains_key("failed"));
}

#[tokio::test]
async fn test_alerts_can_be_previewed_offline_and_missing_media_is_rejected() {
    let (db, _) = database().await;
    let (_status, rx) = watch::channel(StreamStatus {
        state: StreamState::Offline,
    });
    let handle = handle(&db, rx).await;
    let mut receiver = handle.subscribe_alerts();
    let mut alert = alerts::Alert {
        text: "Preview".into(),
        image: String::new(),
        video: String::new(),
        sound: String::new(),
        position: None,
        duration_ms: 1000,
    };
    assert_eq!(
        handle
            .preview_alerts(&db, vec![alert.clone()])
            .await
            .unwrap(),
        1
    );
    assert_eq!(receiver.try_recv().unwrap().text, "Preview");
    alert.image = "missing.png".into();
    assert!(handle.preview_alerts(&db, vec![alert]).await.is_err());
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn media_library_preserves_bytes_and_checks_references() {
    use base64::Engine;
    let (db, _) = database().await;
    let bytes = b"GIF89aexample";
    alerts::upload(&db, "winner.gif".into(), bytes)
        .await
        .unwrap();
    let media = alerts::get(&db, "winner.gif").await.unwrap();
    assert_eq!(media.content_type, "image/gif");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(media.data)
            .unwrap(),
        bytes
    );
    assert!(
        alerts::upload(&db, "winner.gif".into(), bytes)
            .await
            .is_err()
    );
    assert!(
        alerts::upload(&db, "../winner.gif".into(), bytes)
            .await
            .is_err()
    );
    assert!(
        alerts::upload(&db, "script.svg".into(), b"<svg></svg>")
            .await
            .is_err()
    );
    let mut alert = alerts::Alert {
        text: String::new(),
        image: "winner.gif".into(),
        video: String::new(),
        sound: String::new(),
        position: None,
        duration_ms: 1000,
    };
    alerts::validate_media(&db, &[alert.clone()]).await.unwrap();
    alert.sound = "winner.gif".into();
    assert!(alerts::validate_media(&db, &[alert]).await.is_err());
    alerts::upload(&db, "winner.mp4".into(), b"\0\0\0\x18ftypisomexample")
        .await
        .unwrap();
    let mut video_alert = alerts::Alert {
        text: String::new(),
        image: String::new(),
        video: "winner.mp4".into(),
        sound: String::new(),
        position: None,
        duration_ms: 1000,
    };
    assert_eq!(
        alerts::get(&db, "winner.mp4").await.unwrap().content_type,
        "video/mp4"
    );
    alerts::validate_media(&db, &[video_alert.clone()])
        .await
        .unwrap();
    video_alert.image = video_alert.video.clone();
    assert!(alerts::validate_media(&db, &[video_alert]).await.is_err());
}
