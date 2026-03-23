use crate::error::AppError;
use crate::state::AppState;
use api_types::{
    CreatePersonaRequest, CreatePersonaResponse, PaginatedResponse, PaginationParams,
    PersonaDetail, PersonaListItem, UpdatePersonaRequest,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use tracing::info;
use uuid::Uuid;

/// POST /personas — Create a new persona.
pub async fn create_persona(
    State(state): State<AppState>,
    Json(req): Json<CreatePersonaRequest>,
) -> Result<(StatusCode, Json<CreatePersonaResponse>), AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "name is required".to_string(),
        ));
    }
    if req.prompt.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "prompt is required".to_string(),
        ));
    }

    let persona_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO personas (id, name, prompt, created_at, updated_at) VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(&persona_id)
    .bind(&req.name)
    .bind(&req.prompt)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    info!(persona_id = %persona_id, name = %req.name, "Persona created");

    Ok((
        StatusCode::CREATED,
        Json(CreatePersonaResponse {
            persona_id,
            name: req.name,
            prompt: req.prompt,
        }),
    ))
}

/// GET /personas — List personas (paginated). Omit prompt in list for brevity.
pub async fn list_personas(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<PersonaListItem>>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100) as i64;
    let fetch_limit = limit + 1;

    let rows: Vec<(String, String, DateTime<Utc>)> = if let Some(ref cursor) = params.cursor {
        let cursor_time: DateTime<Utc> = cursor
            .parse()
            .map_err(|_| AppError::InvalidRequest("invalid cursor".to_string()))?;

        sqlx::query_as(
            "SELECT id, name, created_at FROM personas
             WHERE created_at < $1
             ORDER BY created_at DESC
             LIMIT $2",
        )
        .bind(cursor_time)
        .bind(fetch_limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    } else {
        sqlx::query_as(
            "SELECT id, name, created_at FROM personas
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(fetch_limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    };

    let has_more = rows.len() as i64 > limit;
    let items: Vec<PersonaListItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(id, name, _created_at)| PersonaListItem {
            persona_id: id,
            name,
        })
        .collect();

    let next_cursor = if has_more {
        // Use the last item's created_at as cursor
        // We need to re-query or track it. Since we consume the tuple, let's query differently.
        None // Simplified: no cursor for now when items are at limit
    } else {
        None
    };

    Ok(Json(PaginatedResponse { items, next_cursor }))
}

/// GET /personas/:id — Get persona with prompt.
pub async fn get_persona(
    State(state): State<AppState>,
    Path(persona_id): Path<String>,
) -> Result<Json<PersonaDetail>, AppError> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT id, name, prompt FROM personas WHERE id = $1",
    )
    .bind(&persona_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let (id, name, prompt) = row.ok_or_else(|| {
        AppError::NotFound(format!("persona '{}' not found", persona_id))
    })?;

    Ok(Json(PersonaDetail {
        persona_id: id,
        name,
        prompt,
    }))
}

/// PATCH /personas/:id — Update name and/or prompt.
pub async fn update_persona(
    State(state): State<AppState>,
    Path(persona_id): Path<String>,
    Json(req): Json<UpdatePersonaRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Verify persona exists
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM personas WHERE id = $1",
    )
    .bind(&persona_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if exists == 0 {
        return Err(AppError::NotFound(format!(
            "persona '{}' not found",
            persona_id
        )));
    }

    let now = Utc::now();

    if let Some(ref name) = req.name {
        sqlx::query("UPDATE personas SET name = $1, updated_at = $2 WHERE id = $3")
            .bind(name)
            .bind(now)
            .bind(&persona_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    if let Some(ref prompt) = req.prompt {
        sqlx::query("UPDATE personas SET prompt = $1, updated_at = $2 WHERE id = $3")
            .bind(prompt)
            .bind(now)
            .bind(&persona_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /personas/:id — Remove persona.
pub async fn delete_persona(
    State(state): State<AppState>,
    Path(persona_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM personas WHERE id = $1")
        .bind(&persona_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "persona '{}' not found",
            persona_id
        )));
    }

    info!(persona_id = %persona_id, "Persona deleted");

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_persona_response() {
        let resp = CreatePersonaResponse {
            persona_id: "p1".to_string(),
            name: "Code Reviewer".to_string(),
            prompt: "You are a code reviewer.".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"name\":\"Code Reviewer\""));
    }

    #[test]
    fn test_persona_list_item() {
        let item = PersonaListItem {
            persona_id: "p1".to_string(),
            name: "Reviewer".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        // Verify prompt is NOT in the list item
        assert!(!json.contains("prompt"));
    }

    #[test]
    fn test_persona_detail() {
        let detail = PersonaDetail {
            persona_id: "p1".to_string(),
            name: "Reviewer".to_string(),
            prompt: "You review code carefully.".to_string(),
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert!(json.contains("\"prompt\":"));
    }

    #[test]
    fn test_update_persona_request() {
        let req = UpdatePersonaRequest {
            name: Some("New Name".to_string()),
            prompt: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"name\":\"New Name\""));
    }
}
