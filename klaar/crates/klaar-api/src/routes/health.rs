//! Sonde de disponibilité (Story 0.5).

use actix_web::{get, HttpResponse};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HealthDto {
    pub status: String,
}

/// Vérifie que le service répond.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "sonde",
    responses((status = 200, description = "Service opérationnel", body = HealthDto))
)]
#[get("/api/v1/health")]
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(HealthDto {
        status: "ok".to_string(),
    })
}
