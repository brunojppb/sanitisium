use std::error::Error;
use std::{sync::Arc, thread::sleep, time::Duration};

use anyhow::Result;
use apalis::prelude::Error as JobError;
use serde::{Deserialize, Serialize};
use tracing::instrument;

#[derive(Debug, Serialize, Deserialize)]
pub struct SanitisePDF {
    id: String,
}

impl SanitisePDF {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

#[derive(Debug)]
pub enum BackgroundJobError {
    InvalidPDF,
}

impl Error for BackgroundJobError {}

impl std::fmt::Display for BackgroundJobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[instrument]
pub async fn sanitise_pdf(job: SanitisePDF) -> Result<(), JobError> {
    let fut = tokio::task::spawn_blocking(move || {
        tracing::info!("Processing PDF. id={}", job.id);
        sleep(Duration::from_secs(5));
        tracing::info!("Processing done. id={}", job.id);
    });

    match fut.await {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::error!("Processing failed. error={e}");
            Err(JobError::Failed(Arc::new(Box::new(
                BackgroundJobError::InvalidPDF,
            ))))
        }
    }
}
