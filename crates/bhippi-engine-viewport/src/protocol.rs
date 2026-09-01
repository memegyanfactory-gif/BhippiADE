//! The JSON-RPC 2.0 control channel protocol (ADR-0020 §child-process model). The app
//! speaks `editor.*` on a stdio loopback; the viewport answers with `editor.*` replies and
//! pushes notification messages. Every request is answered; every answer carries a hint
//! when it fails so the ActivityDock can show a repair step.

use bhippi_engine::transaction::Op;
use bhippi_types::{
    BuildId, EngineActor, EngineTransactionSummary, EntityId, PlayState, SceneId, TransactionId,
};
use serde::{Deserialize, Serialize};
use specta::Type;

/// JSON-RPC 2.0 frame. `id` is an opaque string the caller chooses; notifications have no
/// `id` field. `method` names are dotted (`editor.open_scene`).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Request {
    #[serde(default)]
    pub id: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// The viewport's answer to a `Request`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    Ok {
        id: String,
        result: serde_json::Value,
    },
    Error {
        id: String,
        code: String,
        message: String,
        hint: Option<String>,
    },
}

/// One-way facts the viewport pushes (also mirrored as `EngineEvent`s up the bus).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ViewportNotification {
    Status {
        alive: bool,
        gpu_name: Option<String>,
    },
    Console {
        level: String,
        target: String,
        text: String,
    },
    PlayStats {
        fps: f32,
        frame_ms: f32,
        entities: u32,
        draw_calls: u32,
    },
    PlayStateChanged {
        state: PlayState,
    },
    SelectionChanged {
        entities: Vec<EntityId>,
    },
    TransformsUpdated {
        batch: Vec<bhippi_types::EntityTransformPatch>,
    },
}

/// The editor-side method params the app sends. Mirrors the `EngineAction` surface 1:1 so
/// the pipeline between the AI shell and the viewport stays a pure passthrough.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(untagged)]
pub enum EditorCommandParams {
    /// editor.ping
    Ping,
    /// editor.open_scene { "scene": "<ulid>" }
    OpenScene { scene: SceneId, path: String },
    /// editor.request_scene — returns the current scene document text.
    RequestScene,
    /// editor.apply_ops { "transaction": {…} } — applied atomically, inverse returned.
    ApplyOps { transaction: ViewportTransaction },
    /// editor.set_play_state { "state": "playing"|"paused"|"stop" }
    SetPlayState { state: PlayState },
    /// editor.snapshot — headless-safe status for the status bar.
    Snapshot,
    /// editor.shutdown — polite quit (the app force-kills on timeout).
    Shutdown { reason: String },
}

/// The transaction shape the viewport runs (same serde layout as the engine's own, minus
/// the locally-computed inverse — the viewport computes and returns it).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ViewportTransaction {
    pub id: TransactionId,
    pub label: String,
    pub actor: EngineActor,
    pub ops: Vec<Op>,
}

/// The viewport→editor reply payload for `editor.apply_ops`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ApplyOpsResult {
    pub summary: EngineTransactionSummary,
    pub applied_ops: usize,
}

/// The editor's side of the loopback — how the app names the pipe and asserts protocol
/// versions before first byte (INV-072).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Handshake {
    pub protocol: String,
    pub viewport_bin_version: String,
    pub project_root: String,
    pub scene: Option<SceneId>,
    pub build: Option<BuildId>,
}

impl Handshake {
    /// A valid handshake must pin our protocol version (unknown versions refuse to run).
    #[must_use]
    pub fn is_compatible(protocol: &str) -> bool {
        protocol == crate::PROTOCOL_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplyOpsResult, EditorCommandParams, Handshake, Request, Response, ViewportNotification,
    };

    #[test]
    fn frames_round_trip_through_json() {
        let request = Request {
            id: Some("1".to_owned()),
            method: "editor.open_scene".to_owned(),
            params: serde_json::json!({ "scene": "01JGZWN0R5HE0J0A7B9P8K3Q2X", "path": "x" }),
        };
        let text = serde_json::to_string(&request).expect("serialize");
        let parsed: Request = serde_json::from_str(&text).expect("parse");
        assert_eq!(parsed, request);

        let ok = Response::Ok {
            id: "1".to_owned(),
            result: serde_json::json!({ "hi": true }),
        };
        let text = serde_json::to_string(&ok).expect("serialize");
        let parsed: Response = serde_json::from_str(&text).expect("parse");
        assert_eq!(parsed, ok);
    }

    #[test]
    fn tagged_enums_deserialize_their_variant() {
        let response: Response =
            serde_json::from_str(r#"{"kind":"ok","id":"7","result":{}}"#).expect("ok");
        assert_eq!(
            response,
            Response::Ok {
                id: "7".to_owned(),
                result: serde_json::Value::Object(Default::default())
            }
        );

        let notification: ViewportNotification = serde_json::from_str(
            r#"{"event":"play_stats","fps":60.0,"frame_ms":16.6,"entities":12,"draw_calls":3}"#,
        )
        .expect("notification");
        assert!(matches!(
            notification,
            ViewportNotification::PlayStats { .. }
        ));
    }

    #[test]
    fn handshake_binds_the_protocol_version() {
        assert!(Handshake::is_compatible(crate::PROTOCOL_VERSION));
        assert!(!Handshake::is_compatible("editor.rpc.v999"));
    }

    #[test]
    fn apply_ops_params_round_trip() {
        let apply: EditorCommandParams = serde_json::from_str(
            r#"{"transaction":{"id":"01JGZWN0R5HE0J0A7B9P8K3Q2X","label":"spawn cube","actor":{"kind":"agent"},"ops":[{"op":"spawn","entity":{"id":"01JGZWN0R5HE0J0A7B9P8K3Q3Y","name":"Cube","parent":null,"tags":[],"components":{"Transform":{"pos":[0.0,0.5,0.0]}}},"parent":null}]}}"#,
        )
        .expect("apply params parse");
        assert!(matches!(apply, EditorCommandParams::ApplyOps { .. }));
        let text = serde_json::to_string(&apply).expect("serialize");
        let reparsed: EditorCommandParams = serde_json::from_str(&text).expect("reparse");
        assert_eq!(apply, reparsed);
    }

    #[test]
    fn apply_ops_result_shape_is_stable() {
        let result = ApplyOpsResult {
            summary: super::EngineTransactionSummary {
                label: "spawn".to_owned(),
                actor: super::EngineActor::Agent,
                op_count: 1,
                touched: vec![],
                scene: super::SceneId::new(),
            },
            applied_ops: 1,
        };
        let text = serde_json::to_string(&result).expect("serialize");
        assert!(text.contains("applied_ops"));
    }
}
