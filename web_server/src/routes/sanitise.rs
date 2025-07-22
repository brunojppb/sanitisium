use actix_web::{HttpResponse, Responder, web};
use apalis::prelude::*;
use apalis_sql::sqlite::SqliteStorage;
use tracing::instrument;

use crate::job::SanitisePDF;

#[instrument]
pub async fn enqueue_pdf(worker_storage: web::Data<SqliteStorage<SanitisePDF>>) -> impl Responder {
    let storage = &*worker_storage.into_inner();
    let mut storage = storage.clone();
    match storage.push(SanitisePDF::new("test-fake-pdf".into())).await {
        Ok(_) => HttpResponse::Ok().body("PDF added to queue for processing"),
        Err(e) => {
            tracing::error!("Could not enqueue PDF job. error={e}");
            HttpResponse::BadRequest().body("Error scheduling PDF to be processed")
        }
    }
}
