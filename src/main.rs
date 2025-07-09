#![allow(dead_code)]
use anyhow::Result;
use pdfium_render::prelude::*;
use printpdf::{
    ImageOptimizationOptions, Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, RawImage,
    XObjectTransform,
};
use std::env;
use std::fs::File;
use std::io::{Cursor, Write};
use std::time::Instant;

fn regenerate_pdf(input: &str, output: &str) -> Result<()> {
    const DPI: f32 = 300.0; // Set desired DPI here
    const BATCH_SIZE: usize = 10; // Process pages in batches of 10

    let pdfium = get_pdfium_instance();
    let doc_in = pdfium.load_pdf_from_file(input, None)?;
    let pages = doc_in.pages();
    let total_pages = pages.len() as usize;

    println!(
        "Processing {} pages in batches of {}",
        total_pages, BATCH_SIZE
    );

    let opts = PdfSaveOptions {
        optimize: true,
        secure: true,
        subset_fonts: true,
        image_optimization: Some(ImageOptimizationOptions {
            auto_optimize: Some(true),
            convert_to_greyscale: Some(false),
            dither_greyscale: None,
            max_image_size: None,
            format: None,
            quality: Some(75f32),
        }),
    };

    let mut output_pdf_bytes: Option<Vec<u8>> = None;

    // Process pages in batches to control memory usage
    for batch_start in (0..total_pages).step_by(BATCH_SIZE) {
        let batch_end = std::cmp::min(batch_start + BATCH_SIZE, total_pages);
        println!(
            "Processing batch: pages {} to {}",
            batch_start + 1,
            batch_end
        );

        // Create a new document for this batch only
        let mut doc_batch = PdfDocument::new("Batch Document");
        let mut batch_pages = Vec::with_capacity(batch_end - batch_start);

        // Process pages in current batch
        for index in batch_start..batch_end {
            let page = pages.get(index as u16).unwrap();
            println!("Writing page {} of {}", index + 1, total_pages);

            // Get page size in PDF points (1 point = 1/72 in)
            let width_pts = page.page_size().width().value;
            let height_pts = page.page_size().height().value;

            // Calculate target pixel dimensions for the desired DPI
            let target_render_width = (width_pts * DPI / 72.0).round() as i32;
            let target_render_height = (height_pts * DPI / 72.0).round() as i32;
            println!("Raster");
            // Rasterize the page at the new higher resolution
            let bitmap = page
                .render_with_config(
                    &PdfRenderConfig::new()
                        .set_target_width(target_render_width)
                        .set_target_height(target_render_height)
                        .use_print_quality(true)
                        .set_format(PdfBitmapFormat::BGR),
                )?
                .as_image()
                .to_rgb8();

            println!("Bitmapwrite");
            let mut jpeg_data = Vec::new();
            bitmap.write_to(&mut Cursor::new(&mut jpeg_data), image::ImageFormat::Jpeg)?;

            let mut warnings = Vec::new();
            let image = RawImage::decode_from_bytes(&jpeg_data, &mut warnings).unwrap();
            let image_id = doc_batch.add_image(&image);

            // compute page size *in mm* (printpdf::Mm expects mm)
            let width_mm = Mm(width_pts * 25.4 / 72.0);
            let height_mm = Mm(height_pts * 25.4 / 72.0);

            let contents = vec![Op::UseXobject {
                id: image_id,
                transform: XObjectTransform::default(),
            }];

            println!(
                "Page {}: {}x{} points, {}x{} mm",
                index + 1,
                width_pts,
                height_pts,
                width_mm.0,
                height_mm.0
            );
            let pdf_page = PdfPage::new(width_mm, height_mm, contents);
            batch_pages.push(pdf_page);
            println!("out batch loop");
        }

        println!("saving temp batch");
        // Save this batch to memory (small temporary allocation)
        let mut warnings = Vec::new();
        let batch_bytes = doc_batch.with_pages(batch_pages).save(&opts, &mut warnings);

        // Immediately merge with existing PDF or start new one
        match output_pdf_bytes.take() {
            None => {
                // First batch - this becomes our base
                output_pdf_bytes = Some(batch_bytes);
                println!("First batch processed ({} pages)", batch_end - batch_start);
            }
            Some(existing_bytes) => {
                // Merge with existing and replace
                output_pdf_bytes = Some(merge_pdf_bytes(&existing_bytes, &batch_bytes)?);
                println!(
                    "Batch {} merged ({} total pages so far)",
                    (batch_start / BATCH_SIZE) + 1,
                    batch_end
                );
            }
        }

        // Force cleanup - doc_batch, batch_pages, batch_bytes are dropped here
        // This releases the memory from rasterized bitmaps and temporary data
    }

    // Write final result to disk
    if let Some(final_bytes) = output_pdf_bytes {
        let mut file = File::create(output)?;
        file.write_all(&final_bytes)?;
        println!("Final PDF written to disk");
    }

    Ok(())
}

fn merge_pdf_bytes(existing_pdf: &[u8], new_batch_pdf: &[u8]) -> Result<Vec<u8>> {
    // This function merges two PDF byte arrays by loading them with pdfium,
    // extracting pages, and creating a new merged PDF with printpdf
    let pdfium = get_pdfium_instance();

    // Load existing and new PDFs
    let existing_doc = pdfium.load_pdf_from_byte_slice(existing_pdf, None)?;
    let new_doc = pdfium.load_pdf_from_byte_slice(new_batch_pdf, None)?;

    // Create merged document
    let mut merged_doc = PdfDocument::new("Merged Document");
    let mut all_pages = Vec::new();

    const DPI: f32 = 300.0;

    // Add pages from existing PDF
    for (idx, page) in existing_doc.pages().iter().enumerate() {
        let converted_page = convert_pdfium_page_to_printpdf(&page, &mut merged_doc, DPI)?;
        all_pages.push(converted_page);
        if idx % 50 == 0 && idx > 0 {
            println!("Processed {} existing pages for merge", idx + 1);
        }
    }

    // Add pages from new batch
    for page in new_doc.pages().iter() {
        let converted_page = convert_pdfium_page_to_printpdf(&page, &mut merged_doc, DPI)?;
        all_pages.push(converted_page);
    }

    // Save merged PDF
    let opts = PdfSaveOptions {
        optimize: true,
        secure: true,
        subset_fonts: true,
        image_optimization: Some(ImageOptimizationOptions {
            auto_optimize: Some(true),
            convert_to_greyscale: Some(false),
            dither_greyscale: None,
            max_image_size: None,
            format: None,
            quality: Some(75f32),
        }),
    };

    let mut warnings = Vec::new();
    Ok(merged_doc.with_pages(all_pages).save(&opts, &mut warnings))
}

fn convert_pdfium_page_to_printpdf(
    page: &pdfium_render::prelude::PdfPage,
    doc: &mut PdfDocument,
    dpi: f32,
) -> Result<printpdf::PdfPage> {
    // Get page size in PDF points
    let width_pts = page.page_size().width().value;
    let height_pts = page.page_size().height().value;

    // Calculate target pixel dimensions
    let target_render_width = (width_pts * dpi / 72.0).round() as i32;
    let target_render_height = (height_pts * dpi / 72.0).round() as i32;

    // Rasterize the page
    let bitmap = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(target_render_width)
                .set_target_height(target_render_height)
                .use_print_quality(true)
                .set_format(PdfBitmapFormat::BGR),
        )?
        .as_image()
        .to_rgb8();

    let mut jpeg_data = Vec::new();
    bitmap.write_to(&mut Cursor::new(&mut jpeg_data), image::ImageFormat::Jpeg)?;

    let mut warnings = Vec::new();
    let image = RawImage::decode_from_bytes(&jpeg_data, &mut warnings).unwrap();
    let image_id = doc.add_image(&image);

    // Compute page size in mm
    let width_mm = Mm(width_pts * 25.4 / 72.0);
    let height_mm = Mm(height_pts * 25.4 / 72.0);

    let contents = vec![Op::UseXobject {
        id: image_id,
        transform: XObjectTransform::default(),
    }];

    Ok(printpdf::PdfPage::new(width_mm, height_mm, contents))
}

// For the sake of simplicity, we only Support Mac (ARM64) and Linux (64-bit)
enum SupportArch {
    MacOS,
    Linux,
}

fn _get_pdfium_instance(arch: SupportArch) -> Pdfium {
    let lib_arch = match arch {
        SupportArch::MacOS => "macOS",
        SupportArch::Linux => "linux-x64",
    };

    let lib_path = format!("./pdfium/{}/lib", lib_arch);

    Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&lib_path))
            .or_else(|_| Pdfium::bind_to_system_library())
            .unwrap(),
    )
}

// on MacOS, we need to bind to the library at a specific path
// given that we already include the library in the project
// For experimentation purposes, this is fine.
#[cfg(target_os = "macos")]
fn get_pdfium_instance() -> Pdfium {
    _get_pdfium_instance(SupportArch::MacOS)
}

#[cfg(target_os = "linux")]
fn get_pdfium_instance() -> Pdfium {
    _get_pdfium_instance(SupportArch::Linux)
}

// On other platforms, we can try to use the system library directly.
// It will panic in case PDFium isn't installed.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn get_pdfium_instance() -> Pdfium {
    Pdfium::new(Pdfium::bind_to_system_library())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let filename = &args[1];

    let start_time = Instant::now();
    let output_name = format!("{}_output.pdf", &filename);
    regenerate_pdf(filename, &output_name)?;
    let duration = start_time.elapsed();
    println!(
        "✅ Regenerated PDF saved to {} in {:?}",
        output_name, duration
    );

    Ok(())
}
