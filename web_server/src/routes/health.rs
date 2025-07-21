use actix_web::{HttpResponse, Responder};

pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("Web server is up")
}
