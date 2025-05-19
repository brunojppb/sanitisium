use anyhow::Result;
use pdfium_render::prelude::*;
use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Pt, RawImage, XObjectTransform};
use std::fs::File;
use std::io::{Cursor, Write};

fn regenerate_pdf(input: &str, output: &str) -> Result<()> {
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

        println!("Extracting bitmap w={} h={} pts", width_pts, height_pts);

        // Rasterize the page at exactly one pixel per PDF point
        let bitmap = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(width_pts as i32)
                    .set_target_height(height_pts as i32)
                    .set_format(PdfBitmapFormat::BGRA),
            )?
            .as_image();

        let mut png_data = Vec::new();

        println!("Flusing bitmap");
        bitmap.write_to(&mut Cursor::new(&mut png_data), image::ImageFormat::Png)?;

        let mut warnings = Vec::new();
        let image = RawImage::decode_from_bytes(&png_data, &mut warnings).unwrap();
        let image_id = doc_out.add_image(&image);

        // compute page size *in mm* (printpdf::Mm expects mm)
        let width_mm = Mm(width_pts * 25.4 / 72.0);
        let height_mm = Mm(height_pts * 25.4 / 72.0);
        println!("Push page: w={} h={} mm", width_mm.0, height_mm.0);

        let contents = vec![Op::UseXobject {
            id: image_id,
            transform: XObjectTransform {
                translate_x: Some(Pt(0.0)),
                translate_y: Some(Pt(0.0)),
                rotate: None,
                // no scaling == 1pt in PDF = 1px of our image
                scale_x: Some(1.0),
                scale_y: Some(1.0),
                dpi: Some(72.0),
            },
        }];

        let pdf_page = PdfPage::new(width_mm, height_mm, contents);
        pdf_pages.push(pdf_page);
    }

    println!("Writing final document");
    let mut warnings = Vec::new();
    let pdf_bytes = doc_out
        .with_pages(pdf_pages)
        .save(&PdfSaveOptions::default(), &mut warnings);
    let mut file = File::create(output)?;
    file.write_all(&pdf_bytes)?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn get_pdfium_instance() -> Pdfium {
    Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./pdfium/lib"))
            .or_else(|_| Pdfium::bind_to_system_library())
            .unwrap(), // Or use the ? unwrapping operator to pass any error up to the caller
    )
}

#[cfg(not(target_os = "macos"))]
pub fn get_pdfium_instance() -> Pdfium {
    Pdfium::new(Pdfium::bind_to_system_library())
}

fn main() -> Result<()> {
    for i in 0..10 {
        regenerate_pdf("sample.pdf", &format!("output-{}.pdf", i))?;
        println!("✅ Regenerated PDF saved to output.pdf");
    }

    Ok(())
}
