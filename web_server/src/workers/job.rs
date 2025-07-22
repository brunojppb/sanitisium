use std::error::Error;
use std::{sync::Arc, thread::sleep, time::Duration};

use actix_web::rt::signal;
use anyhow::{Context, Result, anyhow};
use apalis::prelude::Error as JobError;
use apalis::prelude::*;
use apalis_sql::sqlite::SqliteStorage;
use apalis_sql::sqlx::SqlitePool;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::instrument;

use crate::app_settings::AppSettings;
use crate::storage::FileStorage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitisePDFRequest {
    filename: String,
}

impl SanitisePDFRequest {
    pub fn new(filename: String) -> Self {
        Self { filename }
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

#[derive(Debug)]
pub struct SanitisePdfScheduler {
    storage: Mutex<SqliteStorage<SanitisePDFRequest>>,
    monitor: Mutex<Option<Monitor>>,
}

impl SanitisePdfScheduler {
    pub async fn build(settings: AppSettings) -> Result<Self> {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .with_context(|| "Could not connect to SQLite in-memory")
            .unwrap();
        SqliteStorage::setup(&pool)
            .await
            .expect("Could not run sqlite migrations");
        let storage: SqliteStorage<SanitisePDFRequest> = SqliteStorage::new(pool);
        let mutex_storage = Mutex::new(storage.clone());

        let file_storage = FileStorage::new(settings.sanitisation.pdfs_dir);
        let file_storage = Arc::new(file_storage);

        let monitor = Monitor::new().register({
            WorkerBuilder::new("pdf-regenerator")
                .enable_tracing()
                .data(file_storage)
                .concurrency(10)
                .backend(storage)
                .build_fn(sanitise_pdf)
        });

        Ok(Self {
            storage: mutex_storage,
            monitor: Mutex::new(Some(monitor)),
        })
    }

    #[instrument(skip(self))]
    pub async fn enqueue(&self, job: SanitisePDFRequest) -> Result<()> {
        let mut guard = self.storage.lock().await;
        guard
            .push(job.clone())
            .await
            .map_err(|e| anyhow!("Could not enqueue job for processing. job={job:#?} error={e}"))?;

        Ok(())
    }

    pub async fn run_until_stopped(&self) -> std::io::Result<()> {
        let mut guard = self.monitor.lock().await;
        match guard.take() {
            Some(monitor) => monitor.run_with_signal(signal::ctrl_c()).await,
            None => Err(std::io::Error::other(
                "Can only run the job scheduler once in the lifecycle of the server",
            )),
        }
    }
}

#[instrument]
async fn sanitise_pdf(
    job: SanitisePDFRequest,
    data: apalis::prelude::Data<Arc<FileStorage<String>>>,
) -> Result<(), JobError> {
    let fut = tokio::task::spawn_blocking(move || {
        tracing::info!("Processing PDF. filename={}", job.filename);
        sleep(Duration::from_secs(5));
        if let Err(error) = data.delete_file(&job.filename) {
            tracing::error!(
                "Failed to delete file after processing. filename={} error={}",
                job.filename,
                error
            );
        }
        tracing::info!("Processing done. filemame={}", job.filename);
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
