use crate::state::AppState;
use api_types::{HealthResponse, IdleResponse};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

pub async fn ready() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

pub async fn idle(State(state): State<AppState>) -> impl IntoResponse {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM jobs WHERE status IN ('pending', 'assigned')",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    if count == 0 {
        (
            StatusCode::OK,
            Json(IdleResponse {
                idle: true,
                pending_or_assigned_jobs: None,
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(IdleResponse {
                idle: false,
                pending_or_assigned_jobs: Some(count),
            }),
        )
    }
}
