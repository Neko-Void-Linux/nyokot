//! Central `/webhook` endpoint (axum).
//!
//! The RAW body is verified with HMAC-SHA256 BEFORE any JSON parsing —
//! altering a single byte (even whitespace) must invalidate the signature.

use crate::bus::{Bus, BusMsg};
use crate::db::Db;
use crate::github::{parse_event, verify_signature};
use crate::umf::create_envelope;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use serde_json::Value;

#[derive(Clone)]
pub struct WebhookState {
    pub secret: String,
    pub db: Db,
    pub bus: Bus,
}

pub fn router(state: WebhookState) -> Router {
    Router::new()
        .route("/webhook", post(handle))
        .with_state(state)
}

async fn handle(
    State(st): State<WebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    let sig = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !verify_signature(st.secret.as_bytes(), &body, sig) {
        tracing::warn!("[webhook] rejected: bad signature");
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"ok": false})),
        );
    }
    let event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if event == "ping" {
        return (StatusCode::OK, Json(serde_json::json!({"ok": true})));
    }
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"ok": false})),
            )
        }
    };
    let Some(ev) = parse_event(&event, &payload) else {
        return (
            StatusCode::OK,
            Json(serde_json::json!({"ok": true, "ignored": true})),
        );
    };
    // Cache the repo row (best effort) so filters resolve by id.
    let _ = sqlx::query("INSERT OR IGNORE INTO github_repos(repo_id, full_name) VALUES(?1, ?2)")
        .bind(ev.repo_id)
        .bind(&ev.repo_full_name)
        .execute(&st.db)
        .await;
    let targets = match crate::db::resolve_channels(&st.db, ev.kind, ev.repo_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("[webhook] routing query failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"ok": false})),
            );
        }
    };
    let mut dispatched = 0u32;
    for (channel_id, platform) in targets {
        let mut env = match create_envelope(crate::umf::EnvelopeParams {
            platform: "github".into(),
            channel_id,
            user_id: "github".into(),
            username: ev.author.clone(),
            avatar: None,
            kind: ev.kind.into(),
            title: Some(ev.title.clone()),
            text: ev.text.clone(),
            url: Some(ev.url.clone()),
            color: Some(ev.color),
        }) {
            Ok(e) => e,
            Err(_) => continue,
        };
        env.head.trace_path.push(platform.clone());
        if st
            .bus
            .send(BusMsg::Egress {
                target: platform,
                envelope: env,
            })
            .is_ok()
        {
            dispatched += 1;
        }
    }
    tracing::info!(
        "[webhook] {} {} -> {} channel(s)",
        ev.repo_full_name,
        ev.kind,
        dispatched
    );
    (
        StatusCode::OK,
        Json(serde_json::json!({"ok": true, "dispatched": dispatched})),
    )
}
