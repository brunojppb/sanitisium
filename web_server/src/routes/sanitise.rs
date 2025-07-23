use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse, Responder,
    web::{self, Bytes},
};
use tracing::instrument;

use crate::{startup::AppServices, workers::job::SanitisePDFRequest};

#[instrument(skip(_req, body, services))]
pub async fn enqueue_pdf(
    _req: HttpRequest,
    body: Bytes,
    services: web::Data<Arc<AppServices>>,
) -> impl Responder {
    let filename = format!("{}.pdf", uuid::Uuid::new_v4());
    if let Err(error) = &services.file_storage.store_file(&filename, &body) {
        tracing::info!("Could not store PDF file. filename={filename} error={error}");
        return HttpResponse::BadRequest().body("Error while storing PDF file");
    }

    match services
        .job_scheduler
        .enqueue(SanitisePDFRequest::new(filename))
        .await
    {
        Ok(_) => HttpResponse::Ok().body("PDF added to queue for processing"),
        Err(e) => {
            tracing::error!("Could not enqueue PDF job. error={e}");
            HttpResponse::BadRequest().body("Error scheduling PDF to be processed")
        }
    }
}
