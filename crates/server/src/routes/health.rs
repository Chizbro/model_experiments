use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::state::AppState;

pub async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "unavailable", "error": "database not reachable" })),
        ),
    }
}

pub async fn idle(State(state): State<AppState>) -> impl IntoResponse {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE status IN ('pending', 'assigned')",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if count == 0 {
        (StatusCode::OK, Json(json!({ "idle": true, "pending_or_assigned_jobs": 0 })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "idle": false, "pending_or_assigned_jobs": count })),
        )
    }
}
