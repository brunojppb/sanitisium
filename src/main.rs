#![allow(dead_code)]
use anyhow::{Result, anyhow};
use pdfium_render::prelude::*;
use printpdf::{
    ImageOptimizationOptions, Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, RawImage,
    XObjectTransform,
};
use std::cmp::min;
use std::fs::File;
use std::io::{Cursor, Write};
use std::time::Instant;
use std::{env, fs};

use crate::merger::merge_pdf_files;
mod merger;

const PAGE_BATCH: u16 = 5;
const JPG_QUALITY: f32 = 20f32;

pub fn regenerate_pdf(input: &str, output_path: &str) -> Result<()> {
    const DPI: f32 = 300.0; // Set desired DPI here

    let pdfium = get_pdfium_instance();

    let doc_in = pdfium.load_pdf_from_file(input, None)?;
    let pages = doc_in.pages();

    let doc_lenght: u16 = pages.len();

    if doc_lenght == 0 {
        return Err(anyhow::anyhow!("The input PDF has no pages."));
    }

    let capacity = if doc_lenght > PAGE_BATCH {
        PAGE_BATCH
    } else {
        doc_lenght
    };

    let mut acc = 0;
    let mut written_chuncks_count = 0;
    let mut temp_pdf_files: Vec<String> = Vec::new();
    let mut bitmap_container: Option<PdfBitmap> = None;

    while acc < doc_lenght {
        let mut pdf_pages = Vec::with_capacity(capacity as usize);
        let mut doc_out = PdfDocument::new("Regenerated Document");
        let local_acc: u16 = acc;
        // Cap the trailing end of the range at maximum
        // the batch size or how many pages are left in case they are smaller than the batch size
        let top: u16 = local_acc + min(PAGE_BATCH, doc_lenght - local_acc);

        println!("Processing start={local_acc} end={top}");
        for index in local_acc..top {
            let page = pages
                .get(index)
                .map_err(|e| anyhow!("Could not get page at index. {e}"))?;
            // Get page size in PDF points (1 point = 1/72 in)
            // — get the true media‐box in points
            let width_pts = page.page_size().width().value; // f32
            let height_pts = page.page_size().height().value; // f32

            // Calculate target pixel dimensions for the desired DPI
            let target_render_width = (width_pts * DPI / 72.0).round() as i32;
            let target_render_height = (height_pts * DPI / 72.0).round() as i32;

            // Make sure we have a pre-allocated container for the given page dimensions
            match &bitmap_container {
                Some(existing_container)
                    if existing_container.width() == target_render_width
                        && existing_container.height() == target_render_height => {}
                _ => {
                    let new_container = PdfBitmap::empty(
                        target_render_width,
                        target_render_height,
                        PdfBitmapFormat::BGR,
                        pdfium.bindings(),
                    )
                    .expect("Could not create PDFBitmap container for rendering");
                    bitmap_container = Some(new_container);
                }
            };

            let mut rendering_container = bitmap_container.take().unwrap();

            page.render_into_bitmap_with_config(
                &mut rendering_container,
                &PdfRenderConfig::new()
                    .set_target_width(target_render_width)
                    .set_target_height(target_render_height)
                    .set_format(PdfBitmapFormat::BGR),
            )?;

            // Rasterize the page at the new higher resolution
            let bitmap = rendering_container.as_image().to_rgb8();

            let mut jpg_data = Vec::new();

            bitmap.write_to(&mut Cursor::new(&mut jpg_data), image::ImageFormat::Jpeg)?;
            // Put back the reusable rendering container
            // So we can reference it again on the next look run
            bitmap_container = Some(rendering_container);

            let mut warnings = Vec::new();
            let image = RawImage::decode_from_bytes(&jpg_data, &mut warnings).unwrap();
            let image_id = doc_out.add_image(&image);

            // compute page size *in mm* (printpdf::Mm expects mm)
            let width_mm = Mm(width_pts * 25.4 / 72.0);
            let height_mm = Mm(height_pts * 25.4 / 72.0);

            let contents = vec![Op::UseXobject {
                id: image_id,
                transform: XObjectTransform::default(),
            }];

            println!(
                "Page {}: {}x{} points, {}x{} mm",
                index, width_pts, height_pts, width_mm.0, height_mm.0
            );
            let pdf_page = PdfPage::new(width_mm, height_mm, contents);
            pdf_pages.push(pdf_page);
        }
        let mut warnings = Vec::new();

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
                quality: Some(JPG_QUALITY),
            }),
        };

        let pdf_bytes = doc_out.with_pages(pdf_pages).save(&opts, &mut warnings);
        let filename = format!("temp_file_{written_chuncks_count}.pdf");
        let mut file = File::create(&filename)?;
        file.write_all(&pdf_bytes)?;

        temp_pdf_files.push(filename);
        written_chuncks_count += 1;
        acc += PAGE_BATCH
    }

    match merge_pdf_files(&temp_pdf_files, String::from(output_path)) {
        Ok(()) => {
            temp_pdf_files.iter().for_each(|f| {
                if let Err(e) = fs::remove_file(f) {
                    eprintln!("Could not delete temp file. error={e}")
                }
            });
            Ok(())
        }
        Err(e) => {
            eprintln!("Could not merge PDF files. error={e}");
            Err(anyhow!("Error while merging PDF"))
        }
    }
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

    let lib_path = format!("./pdfium/{lib_arch}/lib");

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
// Sorry Windows folks...
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
    println!("✅ Regenerated PDF saved to {output_name} in {duration:?}");

    Ok(())
}
