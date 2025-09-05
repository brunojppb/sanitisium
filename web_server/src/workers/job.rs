use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use actix_web::rt::signal;
use anyhow::{Context, Result};
use apalis::prelude::Error as JobError;
use apalis::prelude::*;
use procspawn::Pool;
use sanitiser::pdf::sanitise::regenerate_pdf;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio::sync::Mutex;
use tracing::instrument;

use crate::app_settings::AppSettings;
use crate::storage::FileStorage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitisePDFRequest {
    filename: String,
    pub id: String,
    pub success_callback_url: String,
    pub failure_callback_url: String,
}

impl SanitisePDFRequest {
    pub fn new(
        filename: String,
        id: String,
        success_callback_url: String,
        failure_callback_url: String,
    ) -> Self {
        Self {
            filename,
            id,
            success_callback_url,
            failure_callback_url,
        }
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
    storage: Mutex<MemoryStorage<SanitisePDFRequest>>,
    monitor: Mutex<Option<Monitor>>,
}

#[derive(Debug)]
pub struct WorkerData {
    storage: FileStorage<String>,
    pool: Pool,
}

impl SanitisePdfScheduler {
    pub async fn build(settings: AppSettings) -> Result<Self> {
        let storage: MemoryStorage<SanitisePDFRequest> = MemoryStorage::new();
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
            .enqueue(job)
            .await
            .map_err(|_| anyhow::anyhow!("Failed to enqueue job"))?;

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
    let client = reqwest::Client::new();
    let inner_job = job.clone();
    let inner_data = data.clone();

    let fut = tokio::task::spawn_blocking(move || {
        tracing::info!("Processing PDF. filename={}", inner_job.filename);
        let original_file = Path::new(inner_data.storage.base_dir()).join(&inner_job.filename);
        let output_file = Path::new(inner_data.storage.base_dir())
            .join(format!("processed_{}", &inner_job.filename));

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
        let proc_handle = procspawn::spawn!(in inner_data.pool, (args) || {
            match regenerate_pdf(&args.original_file, &args.output_file) {
                Ok(()) => {
                    tracing::info!("File regenerated successfully");
                    None
                }
                Err(error) => Some(format!(
                    "Failed to regenerate file. filename={} error={}",
                    args.original_file, error
                )),
            }
        });

        let result = match proc_handle.join() {
            Ok(Some(error_msg)) => {
                tracing::error!(
                    "Failed to sanitise PDF in a background process. error={error_msg}"
                );
                Err(error_msg)
            }
            Ok(None) => {
                tracing::info!("Background process done");
                Ok(output_file)
            }
            Err(error) => {
                let error_msg = format!("Failed to spawn background process. error={error}");
                tracing::error!("{}", error_msg);
                Err(error_msg)
            }
        };

        if let Err(error) = inner_data.storage.delete_file(&inner_job.filename) {
            tracing::error!(
                "Failed to clean-up original file. filename={} error={}",
                inner_job.filename,
                error
            );
        }

        result
    });

    match fut.await {
        Ok(Ok(output_file)) => {
            // Success - send file to success callback
            tracing::info!("Sending success callback for job id={}", &job.id);
            if let Err(e) = send_success_callback(&client, &job, &output_file).await {
                tracing::error!("Failed to send success callback. error={e}");
            }

            if let Some(clean_up_file) = output_file.file_name()
                && let Err(error) = data
                    .storage
                    .delete_file(&clean_up_file.to_str().unwrap().to_string())
            {
                tracing::error!(
                    "Failed to clean-up final output file. filename={clean_up_file:#?} error={error}"
                );
            }

            Ok(())
        }
        Ok(Err(error_msg)) => {
            // PDF processing failed - send error to failure callback
            tracing::info!("Sending failure callback for job id={}", job.id);
            if let Err(e) = send_failure_callback(&client, &job, &error_msg).await {
                tracing::error!("Failed to send failure callback. error={e}");
            }
            Err(JobError::Failed(Arc::new(Box::new(
                BackgroundJobError::InvalidPDF,
            ))))
        }
        Err(e) => {
            // Task execution failed
            let error_msg = format!("Processing task failed. error={e}");
            tracing::error!("{}", error_msg);
            if let Err(e) = send_failure_callback(&client, &job, &error_msg).await {
                tracing::error!("Failed to send failure callback. error={e}");
            }
            Err(JobError::Failed(Arc::new(Box::new(
                BackgroundJobError::InvalidPDF,
            ))))
        }
    }
}

async fn send_success_callback(
    client: &reqwest::Client,
    job: &SanitisePDFRequest,
    output_file: &Path,
) -> Result<(), anyhow::Error> {
    let file_content = std::fs::read(output_file)
        .with_context(|| format!("Failed to read output file: {}", output_file.display()))?;

    let response = client
        .post(&job.success_callback_url)
        .query(&[("id", &job.id)])
        .header("Content-Type", "application/octet-stream")
        .body(file_content)
        .send()
        .await
        .with_context(|| {
            format!(
                "Failed to send success callback to {}",
                job.success_callback_url
            )
        })?;

    if response.status().is_success() {
        tracing::info!(
            "Success callback sent successfully. status={}",
            response.status()
        );
    } else {
        tracing::warn!(
            "Success callback returned non-success status. status={}",
            response.status()
        );
    }

    Ok(())
}

async fn send_failure_callback(
    client: &reqwest::Client,
    job: &SanitisePDFRequest,
    error_message: &str,
) -> Result<(), anyhow::Error> {
    let payload = serde_json::json!({
        "id": job.id,
        "error": error_message
    });

    let response = client
        .post(&job.failure_callback_url)
        .json(&payload)
        .send()
        .await
        .with_context(|| {
            format!(
                "Failed to send failure callback to {}",
                job.failure_callback_url
            )
        })?;

    if response.status().is_success() {
        tracing::info!(
            "Failure callback sent successfully. status={}",
            response.status()
        );
    } else {
        tracing::warn!(
            "Failure callback returned non-success status. status={}",
            response.status()
        );
    }

    Ok(())
}
