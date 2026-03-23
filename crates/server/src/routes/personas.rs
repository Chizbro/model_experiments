use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use api_types::{CreatePersonaRequest, PaginatedResponse, PersonaDetail, PersonaId, PersonaSummary, UpdatePersonaRequest};

use crate::error::AppError;
use crate::state::AppState;

/// POST /personas — create a new persona.
pub async fn create_persona(
    State(state): State<AppState>,
    Json(body): Json<CreatePersonaRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::bad_request("name is required"));
    }
    if body.prompt.trim().is_empty() {
        return Err(AppError::bad_request("prompt is required"));
    }

    let row = sqlx::query_as::<_, (sqlx::types::Uuid, DateTime<Utc>)>(
        "INSERT INTO personas (name, prompt) VALUES ($1, $2) RETURNING id, created_at",
    )
    .bind(body.name.trim())
    .bind(body.prompt.trim())
    .fetch_one(&state.pool)
    .await?;

    let detail = PersonaDetail {
        persona_id: PersonaId::from_string(row.0.to_string()),
        name: body.name.trim().to_string(),
        prompt: body.prompt.trim().to_string(),
        created_at: row.1,
        updated_at: None,
    };

    Ok((StatusCode::CREATED, Json(detail)))
}

#[derive(Debug, Deserialize)]
pub struct ListPersonasQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

/// GET /personas — list personas (name only, no prompt).
pub async fn list_personas(
    State(state): State<AppState>,
    Query(query): Query<ListPersonasQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).min(100);

    let rows = if let Some(ref cursor) = query.cursor {
        let cursor_time: DateTime<Utc> = cursor
            .parse()
            .map_err(|_| AppError::bad_request("Invalid cursor"))?;

        sqlx::query_as::<_, (sqlx::types::Uuid, String, DateTime<Utc>)>(
            "SELECT id, name, created_at FROM personas WHERE created_at < $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(cursor_time)
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, (sqlx::types::Uuid, String, DateTime<Utc>)>(
            "SELECT id, name, created_at FROM personas ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    };

    let has_more = rows.len() as i64 > limit;
    let items: Vec<PersonaSummary> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(id, name, created_at)| PersonaSummary {
            persona_id: PersonaId::from_string(id.to_string()),
            name,
            created_at,
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|item| item.created_at.to_rfc3339())
    } else {
        None
    };

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

/// GET /personas/:id — get persona detail (includes prompt).
pub async fn get_persona(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id_uuid: sqlx::types::Uuid = id
        .parse()
        .map_err(|_| AppError::bad_request("Invalid persona ID"))?;

    let row = sqlx::query_as::<_, (sqlx::types::Uuid, String, String, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, name, prompt, created_at, updated_at FROM personas WHERE id = $1",
    )
    .bind(id_uuid)
    .fetch_optional(&state.pool)
    .await?;

    let (pid, name, prompt, created_at, updated_at) =
        row.ok_or_else(|| AppError::not_found("Persona not found"))?;

    let detail = PersonaDetail {
        persona_id: PersonaId::from_string(pid.to_string()),
        name,
        prompt,
        created_at,
        updated_at: Some(updated_at),
    };

    Ok(Json(detail))
}

/// PATCH /personas/:id — update persona name and/or prompt.
pub async fn update_persona(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdatePersonaRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id_uuid: sqlx::types::Uuid = id
        .parse()
        .map_err(|_| AppError::bad_request("Invalid persona ID"))?;

    if body.name.is_none() && body.prompt.is_none() {
        return Ok(StatusCode::NO_CONTENT);
    }

    // Build dynamic update
    let (query, name_val, prompt_val) = match (&body.name, &body.prompt) {
        (Some(name), Some(prompt)) => (
            "UPDATE personas SET name = $2, prompt = $3, updated_at = now() WHERE id = $1",
            Some(name.trim().to_string()),
            Some(prompt.trim().to_string()),
        ),
        (Some(name), None) => (
            "UPDATE personas SET name = $2, updated_at = now() WHERE id = $1",
            Some(name.trim().to_string()),
            None,
        ),
        (None, Some(prompt)) => (
            "UPDATE personas SET prompt = $2, updated_at = now() WHERE id = $1",
            None,
            Some(prompt.trim().to_string()),
        ),
        (None, None) => unreachable!(),
    };

    let result = match (name_val, prompt_val) {
        (Some(name), Some(prompt)) => {
            sqlx::query(query)
                .bind(id_uuid)
                .bind(name)
                .bind(prompt)
                .execute(&state.pool)
                .await?
        }
        (Some(name), None) => {
            sqlx::query(query)
                .bind(id_uuid)
                .bind(name)
                .execute(&state.pool)
                .await?
        }
        (None, Some(prompt)) => {
            sqlx::query(query)
                .bind(id_uuid)
                .bind(prompt)
                .execute(&state.pool)
                .await?
        }
        (None, None) => unreachable!(),
    };

    if result.rows_affected() == 0 {
        return Err(AppError::not_found("Persona not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /personas/:id — delete a persona.
pub async fn delete_persona(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id_uuid: sqlx::types::Uuid = id
        .parse()
        .map_err(|_| AppError::bad_request("Invalid persona ID"))?;

    let result = sqlx::query("DELETE FROM personas WHERE id = $1")
        .bind(id_uuid)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found("Persona not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}
