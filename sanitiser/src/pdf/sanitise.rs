#![allow(dead_code)]
use pdfium_render::prelude::*;
use printpdf::{
    ImageOptimizationOptions, Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, RawImage,
    XObjectTransform,
};
use std::cmp::min;
use std::fs::File;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::{env, fs};
use thiserror::Error;
use uuid::Uuid;

use crate::pdf::merge::{MergePDFError, merge_pdf_files};

const PAGE_BATCH: u16 = 5;
const JPG_QUALITY: f32 = 70f32;
const DPI: f32 = 300.0;

#[derive(Error, Debug)]
pub enum PDFRegenerationError {
    #[error("Input must be a valid PDF file")]
    InvalidInput,
    #[error("Input file must contain at least one page")]
    EmptyInput,
    #[error(
        "Document contains pages that are too large to be processed. Max width and height are PLACEHOLDER_HERE"
    )]
    PageTooLarge,
    #[error("Could not decode image from page. decoding_error=`{0}`")]
    BadImageDecoding(String),
    #[error("Image rendering container cannot be empty")]
    InvalidImageContainer,
    #[error("Could not convert Bitmap to JPEG")]
    InvalidBitmapToJPG(#[from] image::ImageError),
    #[error("Cannot open file")]
    InvalidFile(#[from] io::Error),
    #[error("Cannot assemble final PDF")]
    BadMerge(#[from] MergePDFError),
    #[error("Cannot manipulate PDF")]
    BadPDF(#[from] PdfiumError),
}

/// Regenerate the input PDF as an entire new file.
/// By taking screenshots of each page of the input file
/// and generating a new PDF, we make sure that the new file
/// is completely sanitised given it is a complete regeneration.
///
/// The trade-off here is that we lose the native PDF objects
/// and JPGs embedded into the final PDF can potentially generate
/// files that are 10x larger.
pub fn regenerate_pdf<P>(input: &P, output_path: &P) -> Result<(), PDFRegenerationError>
where
    P: AsRef<Path>,
{
    let pdfium = get_pdfium_instance();

    let input_filename = input
        .as_ref()
        .file_stem()
        .and_then(|f| f.to_str())
        .ok_or(PDFRegenerationError::InvalidInput)?;

    let input_doc = pdfium.load_pdf_from_file(input, None)?;
    let pages = input_doc.pages();

    let input_doc_length: u16 = pages.len();

    if input_doc_length == 0 {
        return Err(PDFRegenerationError::EmptyInput);
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
    // Unique identifier for prefixing the temporary cache files.
    // This allows us to prevent any clasing in case consumers
    // are sanitizing the same file at once.
    let unique_temp_id = Uuid::new_v4();

    while processed_pages_count < input_doc_length {
        let mut pdf_pages = Vec::with_capacity(chunk_processing_size as usize);
        let mut doc_out = PdfDocument::new("Clean PDF Document");
        let local_acc: u16 = processed_pages_count;
        // Cap the trailing end of the range at maximum the batch size
        // or how many pages are left in case they are smaller than the batch size
        let top: u16 = local_acc + min(PAGE_BATCH, input_doc_length - local_acc);

        for index in local_acc..top {
            let page = pages.get(index)?;
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
                    )?;
                    bitmap_container = Some(new_container);
                }
            };

            let mut rendering_container = bitmap_container
                .take()
                .ok_or(PDFRegenerationError::InvalidImageContainer)?;

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
            let image = RawImage::decode_from_bytes(&jpg_data, &mut warnings)
                .map_err(PDFRegenerationError::BadImageDecoding)?;

            let image_id = doc_out.add_image(&image);

            // compute page size *in mm* (printpdf::Mm expects mm)
            let width_mm = Mm(width_pts * 25.4 / 72.0);
            let height_mm = Mm(height_pts * 25.4 / 72.0);

            let contents = vec![Op::UseXobject {
                id: image_id,
                transform: XObjectTransform::default(),
            }];

            println!("Page {index} regenerated");
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

        let filename =
            format!("{input_filename}_temp_file_{unique_temp_id}_{written_chuncks_count}.pdf");
        let mut temp_file = env::temp_dir();
        temp_file.push(filename);

        let mut file = File::create(&temp_file)?;
        file.write_all(&pdf_bytes)?;

        temp_pdf_files.push(temp_file);
        written_chuncks_count += 1;
        processed_pages_count += PAGE_BATCH
    }

    match merge_pdf_files(&temp_pdf_files, &PathBuf::from(output_path.as_ref())) {
        Ok(()) => {
            // Clean-up the temp files once we generate the final one
            clean_up_temp_files(&temp_pdf_files);
            Ok(())
        }
        Err(e) => {
            clean_up_temp_files(&temp_pdf_files);
            Err(PDFRegenerationError::BadMerge(e))
        }
    }
}

/// Delete the given files
/// Failure to remove them should not halt the process
fn clean_up_temp_files(files: &[PathBuf]) {
    files.iter().for_each(|f| {
        if let Err(e) = fs::remove_file(f) {
            eprintln!("Could not delete temp file. error={e}")
        }
    });
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

    // Make sure that resources/pdfium/<arch>/lib is available in production
    let lib_path = std::env::current_dir().expect("Could not get the current dir path");

    let runtime_lib_path = lib_path
        .join("resources")
        .join("pdfium")
        .join(lib_arch)
        .join("lib");

    // When executing this library from Cargo, we must use
    // resources under the crate's folder
    let mut crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir.pop();
    let crate_dir = crate_dir
        .join("resources")
        .join("pdfium")
        .join(lib_arch)
        .join("lib");

    Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
            &runtime_lib_path,
        ))
        .or_else(|_| {
            println!("Binding to crate dir");
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&crate_dir))
        })
        .or_else(|_| {
            println!("Binding to system");
            Pdfium::bind_to_system_library()
        })
        .unwrap(),
    )
}

// Bind to the library at a specific path during runtime.
// Panics if PDFium isn't available during runtime.
#[cfg(target_os = "macos")]
fn get_pdfium_instance() -> Pdfium {
    _get_pdfium_instance(SupportArch::MacOS)
}

// Bind to the library at a specific path during runtime.
// Panics if PDFium isn't available during runtime.
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

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    use std::fs::File;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    /// Helper for loading test PDF files under the `tests` directory
    fn get_test_pdf_path(filename: &str) -> PathBuf {
        let mut base_path =
            std::env::current_dir().expect("Failed to determine current dir while loading config");

        let crate_name = env!("CARGO_CRATE_NAME");
        if base_path.ends_with(crate_name) {
            base_path.pop();
        }

        base_path.join("resources").join("pdfs").join(filename)
    }

    #[test]
    fn test_regenerate_pdf_single_page() {
        let input = get_test_pdf_path("page-sizes-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = regenerate_pdf(&input, &output_path);
        assert!(
            result.is_ok(),
            "Failed to regenerate single page PDF: {result:?}"
        );

        // Verify the output file exists and is a valid PDF
        assert!(output_path.exists(), "Output file does not exist");
        let output_file = File::open(&output_path).expect("Failed to open output file");
        let output_doc = Document::load_from(output_file);
        assert!(
            output_doc.is_ok(),
            "Output is not a valid PDF: {output_doc:?}"
        );

        // Load the original to compare page count
        let original_file = File::open(&input).expect("Failed to open original file");
        let original_doc = Document::load_from(original_file).expect("Failed to load original PDF");
        let original_pages = original_doc.get_pages();

        let regenerated_doc = output_doc.unwrap();
        let regenerated_pages = regenerated_doc.get_pages();

        assert_eq!(
            original_pages.len(),
            regenerated_pages.len(),
            "Regenerated PDF should have the same number of pages as original"
        );
    }

    #[test]
    fn test_regenerate_pdf_multiple_pages() {
        let input = get_test_pdf_path("annotations-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = regenerate_pdf(&input, &output_path);
        assert!(
            result.is_ok(),
            "Failed to regenerate multi-page PDF: {result:?}"
        );

        // Verify the output file exists and is a valid PDF
        assert!(output_path.exists(), "Output file does not exist");
        let output_file = File::open(&output_path).expect("Failed to open output file");
        let output_doc = Document::load_from(output_file);
        assert!(output_doc.is_ok(), "Output is not a valid PDF");

        // Load the original to compare page count
        let original_file = File::open(&input).expect("Failed to open original file");
        let original_doc = Document::load_from(original_file).expect("Failed to load original PDF");
        let original_pages = original_doc.get_pages();

        let regenerated_doc = output_doc.unwrap();
        let regenerated_pages = regenerated_doc.get_pages();

        assert_eq!(
            original_pages.len(),
            regenerated_pages.len(),
            "Regenerated PDF should have the same number of pages as original"
        );
    }

    #[test]
    fn test_regenerate_pdf_with_images() {
        let input = get_test_pdf_path("image-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = regenerate_pdf(&input, &output_path);
        assert!(
            result.is_ok(),
            "Failed to regenerate PDF with images: {result:?}"
        );

        assert!(output_path.exists(), "Output file does not exist");
        let output_file = File::open(&output_path).expect("Failed to open output file");
        let output_doc = Document::load_from(output_file);
        assert!(output_doc.is_ok(), "Output is not a valid PDF");

        let original_file = File::open(&input).expect("Failed to open original file");
        let original_doc = Document::load_from(original_file).expect("Failed to load original PDF");
        let original_pages = original_doc.get_pages();

        let regenerated_doc = output_doc.unwrap();
        let regenerated_pages = regenerated_doc.get_pages();

        assert_eq!(
            original_pages.len(),
            regenerated_pages.len(),
            "Regenerated PDF should have the same number of pages as original"
        );
    }

    #[test]
    fn test_regenerate_pdf_file_size_comparison() {
        let input = get_test_pdf_path("export-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = regenerate_pdf(&input, &output_path);
        assert!(result.is_ok(), "Failed to regenerate PDF: {result:?}");

        let original_size = std::fs::metadata(&input)
            .expect("Failed to get original file size")
            .len();
        let regenerated_size = std::fs::metadata(&output_path)
            .expect("Failed to get regenerated file size")
            .len();

        // The regenerated file should exist and have some content.
        // The rengerated file is generally 10x larger than the original one
        // So there is no point in comparing exact file sizes
        assert!(
            regenerated_size > original_size,
            "Regenerated file should be larger than original file"
        );
    }

    #[test]
    fn test_regenerate_pdf_nonexistent_file() {
        let nonexistent = PathBuf::from("/path/that/does/not/exist.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = regenerate_pdf(&nonexistent, &output_path);
        assert!(result.is_err(), "Should return error for nonexistent file");
    }

    #[test]
    fn test_regenerate_pdf_invalid_output_path() {
        let input = get_test_pdf_path("page-sizes-test.pdf");
        let invalid_output = PathBuf::from("/invalid/directory/that/does/not/exist/output.pdf");

        let result = regenerate_pdf(&input, &invalid_output);
        assert!(
            result.is_err(),
            "Should return error for invalid output path"
        );
    }

    #[test]
    fn test_regenerate_pdf_output_structure() {
        let input = get_test_pdf_path("annotations-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        regenerate_pdf(&input, &output_path).expect("Failed to regenerate PDF");

        let output_file = File::open(&output_path).expect("Failed to open output file");
        let regenerated_doc =
            Document::load_from(output_file).expect("Failed to load regenerated PDF");

        let catalog = regenerated_doc.catalog();
        assert!(
            catalog.is_ok(),
            "Regenerated PDF should have a valid catalog"
        );

        let catalog = catalog.unwrap();
        let pages_ref = catalog.get(b"Pages").and_then(|p| p.as_reference());
        assert!(
            pages_ref.is_ok(),
            "Catalog should reference a valid Pages object"
        );

        let pages_id = pages_ref.unwrap();
        let pages_obj = regenerated_doc.get_object(pages_id);
        assert!(pages_obj.is_ok(), "Should be able to get Pages object");

        let pages_dict = pages_obj.unwrap().as_dict();
        assert!(pages_dict.is_ok(), "Pages object should be a dictionary");

        let pages_dict = pages_dict.unwrap();

        assert!(pages_dict.has(b"Count"), "Pages object should have Count");
        assert!(
            pages_dict.has(b"Kids"),
            "Pages object should have Kids array"
        );

        if let (Ok(count), Ok(kids)) = (
            pages_dict.get(b"Count").and_then(|c| c.as_i64()),
            pages_dict.get(b"Kids").and_then(|k| k.as_array()),
        ) {
            assert_eq!(
                count as usize,
                kids.len(),
                "Page count should match kids array length"
            );
        }
    }

    #[test]
    fn test_regenerate_pdf_temp_files_cleanup() {
        let input = get_test_pdf_path("page-sizes-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        // Get the filename stem to predict temp file names
        let input_filename = input.file_stem().and_then(|f| f.to_str()).unwrap();

        // Look for any existing temp files before running the function
        let current_dir = std::env::current_dir().expect("Failed to get current directory");
        let temp_file_pattern = format!("{input_filename}_temp_file_");

        let result = regenerate_pdf(&input, &output_path);
        assert!(result.is_ok(), "Failed to regenerate PDF: {result:?}");

        // After successful execution, temp files should be cleaned up
        // Check current directory for any remaining temp files
        if let Ok(entries) = std::fs::read_dir(&current_dir) {
            for entry in entries.flatten() {
                let filename = entry.file_name();
                if let Some(name) = filename.to_str() {
                    assert!(
                        !name.starts_with(&temp_file_pattern),
                        "Temp file {name} should have been cleaned up"
                    );
                }
            }
        }
    }

    #[test]
    fn test_regenerate_pdf_document_integrity() {
        // Test with a PDF that should trigger batch processing
        // This tests the chunking logic when processing multiple pages
        let input = get_test_pdf_path("image-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = regenerate_pdf(&input, &output_path);
        assert!(
            result.is_ok(),
            "Failed to regenerate PDF with batch processing: {result:?}"
        );

        let output_file = File::open(&output_path).expect("Failed to open output file");
        let output_doc = Document::load_from(output_file);
        assert!(output_doc.is_ok(), "Batch processed PDF should be valid");

        let sanitised_sample_file = get_test_pdf_path("image-test-sanitised.pdf");

        let sanitised_file =
            File::open(&sanitised_sample_file).expect("Failed to open sample file");
        let sample_doc = Document::load_from(sanitised_file).expect("Failed to load sample PDF");
        let sample_pages = sample_doc.get_pages();

        let regenerated_doc = output_doc.unwrap();
        let regenerated_pages = regenerated_doc.get_pages();

        assert_eq!(
            sample_pages, regenerated_pages,
            "Batch processed PDF should pass integrity check with pre-sanitised file"
        );
    }
}
