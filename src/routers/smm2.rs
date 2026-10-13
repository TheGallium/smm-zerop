use std::collections::HashMap;

use axum::{
    Form, Json, Router,
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use minijinja::context;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tower_http::cors::{self, CorsLayer};

use crate::{
    components::{app_state::AppState, discord_webhook},
    entities::smm2_level::{self, Smm2Level},
    errors::ResponseError,
};

/// Note to self: the API routes used to be on a subdomain, and are now in a
/// subdirectory, so this needs to be redirected (or proxied) in prod for
/// compatibility.
pub fn build() -> Router<AppState> {
    let cors_layer = CorsLayer::new()
        .allow_headers(cors::Any)
        .allow_methods(cors::Any)
        .allow_origin(cors::Any);
    let api_router = Router::new()
        .route("/api/smm2/random_level", get(api_random_level))
        .route("/api/smm2/mark_cleared", post(api_mark_cleared))
        .layer(cors_layer);

    Router::new()
        .route("/smm2/random_level/", get(random_level))
        .route("/smm2/mark_cleared/", post(mark_cleared))
        .merge(api_router)
}

#[derive(Debug, Deserialize, Serialize)]
struct ExtraRandomLevelParams {
    #[serde(default)]
    mark_clear_success: bool,
}

#[axum::debug_handler]
#[tracing::instrument(skip(app_state))]
async fn random_level(
    Query(filter_params): Query<smm2_level::FilterParams>,
    Query(extra_params): Query<ExtraRandomLevelParams>,
    State(app_state): State<AppState>,
) -> Result<Response, ResponseError> {
    // Happily special-casing the year parameter...
    // - If no year is set, let's make a decision for the user and default them
    //   to the current focus year. This could be a config, but I'll be adding
    //   a changelog entry for the year flip anyway, so this is fine.
    // - However, if we always set a year if it's not provided, we have to add
    //   a special-case for the UI to indicate "any". I picked -1, because why
    //   not, so we have to unset the parameter so internal processing works.
    let mut effective_filters = filter_params.clone();
    effective_filters.year = match effective_filters.year {
        None => Some(2024),
        Some(-1) => None,
        Some(year) => Some(year),
    };

    let current_filter_query = serde_urlencoded::to_string(filter_params)
        .map_err(|e| ResponseError::InternalError(e.to_string()))?;

    let level = Smm2Level::get_random_level(&app_state.database, &effective_filters).await?;
    Ok(Html(
        app_state
            .template
            .acquire_env()
            .get_template("smm2/random_level.html")?
            .render(context! {
                current_filter_query,
                effective_filters,
                extra_params => minijinja::value::Serde(&extra_params),
                level
            })?,
    )
    .into_response())
}

#[axum::debug_handler]
#[tracing::instrument(skip(app_state))]
async fn api_random_level(
    Query(params): Query<smm2_level::FilterParams>,
    State(app_state): State<AppState>,
) -> Result<Response, ResponseError> {
    let random_level_result = Smm2Level::get_random_level(&app_state.database, &params).await?;

    if let Some(result) = random_level_result {
        Ok(Json(result).into_response())
    } else {
        Err(ResponseError::NotFoundError())
    }
}

#[derive(Debug, Deserialize)]
struct PostSmm2MarkClearedPayload {
    current_filter_query: Option<String>,
    level_id: String,
    source: Option<String>,
}

#[axum::debug_handler]
#[tracing::instrument(skip(app_state))]
async fn mark_cleared(
    State(app_state): State<AppState>,
    Form(payload): Form<PostSmm2MarkClearedPayload>,
) -> Result<Response, ResponseError> {
    if !Smm2Level::id_exists(&app_state.database, &payload.level_id).await {
        return Err(ResponseError::NotFoundError());
    }

    discord_webhook::post_clear(
        &app_state,
        &Smm2Level::formatted_level_id(&payload.level_id),
        payload.source.as_deref(),
    )
    .await
    .map_err(|e| ResponseError::InternalError(e.to_string()))?;

    let redirect = if let Some(original_query) = payload.current_filter_query {
        let mut query: HashMap<String, String> = serde_urlencoded::from_str(&original_query)
            .map_err(|_| ResponseError::BadRequest("invalid current_filter_query".to_string()))?;

        query.insert("mark_clear_success".to_string(), "true".to_string());
        Redirect::to(&format!(
            "/smm2/random_level/?{}",
            serde_urlencoded::to_string(query)
                .expect("query params to never be invalid at this point")
        ))
    } else {
        Redirect::to("/smm2/random_level/")
    };

    Ok(redirect.into_response())
}

#[axum::debug_handler]
#[tracing::instrument(skip(app_state))]
async fn api_mark_cleared(
    State(app_state): State<AppState>,
    Json(payload): Json<PostSmm2MarkClearedPayload>,
) -> Result<Response, ResponseError> {
    let normalized_id = Smm2Level::normalized_internal_level_id(&payload.level_id);

    if normalized_id.len() != 9 {
        return Err(ResponseError::BadRequest("invalid level id".to_string()));
    }

    if !Smm2Level::id_exists(&app_state.database, &normalized_id).await {
        return Err(ResponseError::NotFoundError());
    }

    match discord_webhook::post_clear(
        &app_state,
        &Smm2Level::formatted_level_id(&normalized_id),
        payload.source.as_deref(),
    )
    .await
    {
        Ok(_) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(msg) => Err(ResponseError::InternalError(msg.to_string())),
    }
}
