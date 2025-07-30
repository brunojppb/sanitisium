use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse, Responder,
    web::{self, Bytes, Query},
};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{startup::AppServices, workers::job::SanitisePDFRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitisePDFRequestArgs {
    pub id: String,
    pub success_callback_url: String,
    pub failure_callback_url: String,
}

#[instrument(skip(_req, body, services))]
pub async fn enqueue_pdf(
    _req: HttpRequest,
    body: Bytes,
    query: Query<SanitisePDFRequestArgs>,
    services: web::Data<Arc<AppServices>>,
) -> impl Responder {
    let filename = format!("{}.pdf", uuid::Uuid::new_v4());
    if let Err(error) = &services.file_storage.store_file(&filename, &body) {
        tracing::info!("Could not store PDF file. filename={filename} error={error}");
        return HttpResponse::BadRequest().body("Error while storing PDF file");
    }

    let request_args = query.into_inner();
    match services
        .job_scheduler
        .enqueue(SanitisePDFRequest::new(
            filename,
            request_args.id,
            request_args.success_callback_url,
            request_args.failure_callback_url,
        ))
        .await
    {
        Ok(_) => HttpResponse::Ok().body("PDF added to queue for processing"),
        Err(e) => {
            tracing::error!("Could not enqueue PDF job. error={e}");
            HttpResponse::BadRequest().body("Error scheduling PDF to be processed")
        }
    }
}
