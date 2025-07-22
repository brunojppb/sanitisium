use anyhow::Result;
use clap::Parser;
use sanitiser::pdf::sanitise::regenerate_pdf;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "sanitisium-cli")]
#[command(about = "Tool for regenerating PDFs")]
#[command(version)]
struct Args {
    /// Path to the input PDF file to sanitise
    #[arg(help = "The PDF file to sanitise")]
    input: PathBuf,

    /// Path to the output PDF file (optional)
    #[arg(
        short,
        long,
        help = "Output path for the sanitised PDF. Defaults to the input filename prefixed with 'regenerated_'"
    )]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    init_tracing()?;
    let args = Args::parse();

    let output_path = match args.output {
        Some(path) => path,
        None => {
            let input_path = &args.input;
            let parent_dir = input_path.parent().unwrap_or(Path::new("."));
            let file_stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("sanitised");
            let extension = input_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("pdf");

            parent_dir.join(format!("regenerated_{file_stem}.{extension}"))
        }
    };

    let start_time = Instant::now();
    regenerate_pdf(&args.input, &output_path)?;

    let duration = start_time.elapsed();
    tracing::info!(
        "Regenerated PDF saved to {} in {:?}",
        output_path.display(),
        duration
    );

    Ok(())
}

fn init_tracing() -> Result<()> {
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);
    let filter_layer =
        EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("debug"))?;
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();

    Ok(())
}
