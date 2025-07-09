use anyhow::Result;
use pdfium_render::prelude::*;
use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Pt, RawImage, XObjectTransform};
use std::env;
use std::fs::File;
use std::io::{Cursor, Write};
use std::time::Instant;

fn regenerate_pdf(input: &str, output: &str) -> Result<()> {
    const DPI: f32 = 300.0; // Set desired DPI here

    let pdfium = get_pdfium_instance();

    let doc_in = pdfium.load_pdf_from_file(input, None)?;
    let pages = doc_in.pages();

    let mut doc_out = PdfDocument::new("Regenerated Document");

    let mut pdf_pages = Vec::with_capacity(pages.len() as usize);
    for (index, page) in pages.iter().enumerate() {
        println!("Writing page {} of {}", index, pages.len());
        // Get page size in PDF points (1 point = 1/72 in)
        // — get the true media‐box in points
        let width_pts = page.page_size().width().value; // f32
        let height_pts = page.page_size().height().value; // f32

        // Calculate target pixel dimensions for the desired DPI
        let target_render_width = (width_pts * DPI / 72.0).round() as i32;
        let target_render_height = (height_pts * DPI / 72.0).round() as i32;

        // Rasterize the page at the new higher resolution
        let bitmap = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(target_render_width)
                    .set_target_height(target_render_height)
                    .use_print_quality(true)
                    .set_format(PdfBitmapFormat::BGRA),
            )?
            .as_image();

        let mut png_data = Vec::new();

        bitmap.write_to(&mut Cursor::new(&mut png_data), image::ImageFormat::Png)?;

        let mut warnings = Vec::new();
        let image = RawImage::decode_from_bytes(&png_data, &mut warnings).unwrap();
        let image_id = doc_out.add_image(&image);

        // compute page size *in mm* (printpdf::Mm expects mm)
        let width_mm = Mm(width_pts * 25.4 / 72.0);
        let height_mm = Mm(height_pts * 25.4 / 72.0);

        let contents = vec![Op::UseXobject {
            id: image_id,
            transform: XObjectTransform {
                translate_x: Some(Pt(0.0)),
                translate_y: Some(Pt(0.0)),
                rotate: None,
                scale_x: Some(1f32),
                scale_y: Some(1f32),
                dpi: Some(DPI), // Assume base image DPI is 72, then scale it
            },
        }];

        println!(
            "Page {}: {}x{} points, {}x{} mm",
            index, width_pts, height_pts, width_mm.0, height_mm.0
        );
        let pdf_page = PdfPage::new(width_mm, height_mm, contents);
        pdf_pages.push(pdf_page);
    }

    let mut warnings = Vec::new();

    let pdf_bytes = doc_out
        .with_pages(pdf_pages)
        .save(&PdfSaveOptions::default(), &mut warnings);
    let mut file = File::create(output)?;
    file.write_all(&pdf_bytes)?;
    Ok(())
}

// on MacOS, we need to bind to the library at a specific path
// given that we already include the library in the project
// For experimentation purposes, this is fine.
#[cfg(target_os = "macos")]
pub fn get_pdfium_instance() -> Pdfium {
    Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./pdfium/lib"))
            .or_else(|_| Pdfium::bind_to_system_library())
            .unwrap(),
    )
}

// On other platforms, we can use the system library directly

#[cfg(not(target_os = "macos"))]
pub fn get_pdfium_instance() -> Pdfium {
    Pdfium::new(Pdfium::bind_to_system_library())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let filename = &args[1];

    let start_time = Instant::now();
    let output_name = format!("output.pdf");
    regenerate_pdf(filename, &output_name)?;
    let duration = start_time.elapsed();
    println!(
        "✅ Regenerated PDF saved to {} in {:?}",
        output_name, duration
    );

    Ok(())
}
