use anyhow::Result;

use std::env;
use std::time::Instant;

mod pdf;

use crate::pdf::sanitise::regenerate_pdf;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let filename = &args[1];

    let start_time = Instant::now();
    let output_name = format!("{}_output.pdf", &filename);
    regenerate_pdf(filename, &output_name)?;
    let duration = start_time.elapsed();
    println!("✅ Regenerated PDF saved to {output_name} in {duration:?}");

    Ok(())
}
