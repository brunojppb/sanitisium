use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use actix_web::rt::signal;
use anyhow::{Context, Result, anyhow};
use apalis::prelude::Error as JobError;
use apalis::prelude::*;
use apalis_sql::sqlite::SqliteStorage;
use apalis_sql::sqlx::SqlitePool;
use procspawn::Pool;
use sanitiser::pdf::sanitise::regenerate_pdf;
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

#[derive(Debug)]
pub struct WorkerData {
    storage: FileStorage<String>,
    pool: Pool,
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
        let pool = Pool::new(10).expect("Could not create pool of background processes");
        let worker_data = Arc::new(WorkerData {
            storage: file_storage,
            pool,
        });

        let monitor = Monitor::new().register({
            WorkerBuilder::new("pdf-regenerator")
                .enable_tracing()
                .data(worker_data)
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

#[derive(Debug, Serialize, Deserialize)]
struct ProcData {
    original_file: String,
    output_file: String,
}

#[instrument(skip(data))]
async fn sanitise_pdf(
    job: SanitisePDFRequest,
    data: apalis::prelude::Data<Arc<WorkerData>>,
) -> Result<(), JobError> {
    let fut = tokio::task::spawn_blocking(move || {
        tracing::info!("Processing PDF. filename={}", job.filename);
        let original_file = Path::new(data.storage.base_dir()).join(&job.filename);
        let output_file =
            Path::new(data.storage.base_dir()).join(format!("processed_{}", &job.filename));

        let args = ProcData {
            original_file: original_file.to_str().unwrap().into(),
            output_file: output_file.to_str().unwrap().into(),
        };

        // The C++ PDF handling library we use isn't thread-safe,
        // So the Rust wrapper (pdfium-render) puts a mutex behind the
        // C++ bindings to avoid segfaulting.
        // This means that running PDFium in multiple threads won't help
        // us to process multiple files at once.
        //
        // PDFium works better with its own deficated process.
        // That way, it won't be able to access shared memory.
        //
        // By using procspawn, we are able to fork a child process
        // and use PDFium isolated for each task.
        //
        // This is probably more costly, but we can improve this later
        // with a process pool that can be reused across tasks.
        let proc_handle = procspawn::spawn!(in data.pool, (args) || {
            match regenerate_pdf(&args.original_file, &args.output_file) {
                Ok(()) => {
                    tracing::info!("File regenerated successfuly");
                    None
                }
                Err(error) => Some(format!(
                    "Failed to regenerate file. filename={} error={}",
                    args.original_file, error
                )),
            }
        });

        match proc_handle.join() {
            Ok(Some(error_msg)) => {
                tracing::error!("Failed to sanitise PDF in a background process. error={error_msg}")
            }
            Ok(None) => {
                tracing::info!("Background process done");
            }
            Err(error) => {
                tracing::error!("To spawn background process. error={}", error);
            }
        };

        if let Err(error) = data.storage.delete_file(&job.filename) {
            tracing::error!(
                "Failed to clean-up file. filename={} error={}",
                job.filename,
                error
            );
        }
    });

    match fut.await {
        Ok(()) => {
            tracing::info!("Worker done");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Processing failed. error={e}");
            Err(JobError::Failed(Arc::new(Box::new(
                BackgroundJobError::InvalidPDF,
            ))))
        }
    }
}
