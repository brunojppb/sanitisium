#![allow(dead_code)]
use anyhow::{Result, anyhow};
use pdfium_render::prelude::*;
use printpdf::{
    ImageOptimizationOptions, Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, RawImage,
    XObjectTransform,
};
use std::cmp::min;
use std::fs;
use std::fs::File;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use crate::pdf::merge::merge_pdf_files;

const PAGE_BATCH: u16 = 5;
const JPG_QUALITY: f32 = 70f32;
const DPI: f32 = 300.0;

/// Regenerate the input PDF as an entire new file.
/// By taking screenshots of each page of the input file
/// and generating a new PDF, we make sure that the new file
/// is completely sanitised given it is a complete regeneration.
///
/// The trade-off here is that we lose the native PDF objects
/// and JPGs embedded into the final PDF can potentially generate
/// files that are 10x larger.
pub fn regenerate_pdf<P>(input: &P, output_path: &P) -> Result<()>
where
    P: AsRef<Path>,
{
    let pdfium = get_pdfium_instance();

    let input_filename = input
        .as_ref()
        .file_stem()
        .and_then(|f| f.to_str())
        .ok_or(anyhow!("Invalid input file"))?;

    let input_doc = pdfium.load_pdf_from_file(input, None)?;
    let pages = input_doc.pages();

    let input_doc_length: u16 = pages.len();

    if input_doc_length == 0 {
        return Err(anyhow::anyhow!("The input PDF has no pages."));
    }

    // We process at most the PAGE_BATCH page count at each loop iteration.
    // In case the original PDF is smaller than that, then we can fallback to its page count.
    // This is necessary so we keep memory pressure low across all threads using this function
    let chunk_processing_size = if input_doc_length > PAGE_BATCH {
        PAGE_BATCH
    } else {
        input_doc_length
    };

    let mut processed_pages_count = 0;
    let mut written_chuncks_count = 0;
    let mut temp_pdf_files: Vec<_> = Vec::new();
    let mut bitmap_container: Option<PdfBitmap> = None;

    while processed_pages_count < input_doc_length {
        let mut pdf_pages = Vec::with_capacity(chunk_processing_size as usize);
        let mut doc_out = PdfDocument::new("Clean PDF Document");
        let local_acc: u16 = processed_pages_count;
        // Cap the trailing end of the range at maximum the batch size
        // or how many pages are left in case they are smaller than the batch size
        let top: u16 = local_acc + min(PAGE_BATCH, input_doc_length - local_acc);

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

            let mut rendering_container = bitmap_container
                .take()
                .ok_or(anyhow!("Bitmap container cannot be empty"))?;

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
            // So we can reference it again on the next loop run
            // preventing allocating another buffer
            bitmap_container = Some(rendering_container);

            let mut warnings = Vec::new();
            let image = RawImage::decode_from_bytes(&jpg_data, &mut warnings).map_err(|e| {
                anyhow!("Could not decode image from bytes on page {index} error={e}")
            })?;

            let image_id = doc_out.add_image(&image);

            // compute page size *in mm* (printpdf::Mm expects mm)
            let width_mm = Mm(width_pts * 25.4 / 72.0);
            let height_mm = Mm(height_pts * 25.4 / 72.0);

            let contents = vec![Op::UseXobject {
                id: image_id,
                transform: XObjectTransform::default(),
            }];

            println!("Page {} regenerated", index);
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
                format: Some(printpdf::ImageCompression::Jpeg),
                quality: Some(JPG_QUALITY),
            }),
        };

        let pdf_bytes = doc_out.with_pages(pdf_pages).save(&opts, &mut warnings);

        // @TODO: We should probably take a temp directory
        // to use it as a container for all temp files
        let filename = format!("{input_filename}_temp_file_{written_chuncks_count}.pdf");
        let temp_path = PathBuf::from(filename);
        let mut file = File::create(&temp_path)?;
        file.write_all(&pdf_bytes)?;

        temp_pdf_files.push(temp_path);
        written_chuncks_count += 1;
        processed_pages_count += PAGE_BATCH
    }

    match merge_pdf_files(&temp_pdf_files, &PathBuf::from(output_path.as_ref())) {
        Ok(()) => {
            // Clean-up the temp files once we generate the final one
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

// For the sake of simplicity, we only Support Mac (ARM64) and Linux (AMD 64-bit)
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
