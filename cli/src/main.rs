use anyhow::Result;
use clap::Parser;
use sanitiser::pdf::sanitise::regenerate_pdf;

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

            parent_dir.join(format!("regenerated_{}.{}", file_stem, extension))
        }
    };

    let start_time = Instant::now();
    regenerate_pdf(
        args.input.to_str().expect("Invalid input file path"),
        output_path.to_str().expect("Invalid output file path"),
    )?;

    let duration = start_time.elapsed();
    println!(
        "✅ Regenerated PDF saved to {} in {:?}",
        output_path.display(),
        duration
    );

    Ok(())
}
