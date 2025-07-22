use actix_web::{HttpResponse, Responder};
use tracing::instrument;

#[instrument]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("Web server is up")
}
