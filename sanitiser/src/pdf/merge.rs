use anyhow::Error;
use lopdf::{Document, Object};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;
use std::time::Instant;

/// Merge every file in `inputs` into a single PDF file at `output_path`.
/// The first file becomes the "base"; all others are appended.
///
/// Implementation inspired on the reference example from the [lopdf repo here.](https://github.com/J-F-Liu/lopdf/blob/c320c1d9d90028ee64e668f0bbbe9815fae3fb44/examples/merge.rs)
pub fn merge_pdf_files<P>(files: &[P], output_path: P) -> Result<(), Error>
where
    P: AsRef<Path>,
{
    if files.is_empty() {
        return Err(anyhow::anyhow!("No input files provided"));
    }

    let start_time = Instant::now();

    // Start with the first document as the base
    let first_path = &files[0];
    let first_file = File::open(first_path.as_ref())?;
    let mut merged_doc = Document::load_from(first_file)?;

    if files.len() == 1 {
        // Only one file, just save it to the given output and bail
        merged_doc.save(output_path.as_ref())?;
        return Ok(());
    }

    // Track the next available object ID
    let mut max_id = merged_doc.max_id + 1;

    let mut all_pages = BTreeMap::new();
    let mut all_objects = BTreeMap::new();

    // Add all pages from the base document
    // to our accumulator
    let base_pages = merged_doc.get_pages();
    for (_, page_id) in base_pages {
        all_pages.insert(page_id, merged_doc.get_object(page_id)?.clone());
    }

    for input_path in files.iter().skip(1) {
        let file = File::open(input_path.as_ref())?;
        let mut doc = Document::load_from(file)?;

        // Renumber objects to avoid conflicts
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        let pages = doc.get_pages();

        // Now we should get all pages from each document
        // and add it to our final container collection
        for (_, page_id) in pages {
            all_pages.insert(page_id, doc.get_object(page_id)?.clone());
        }

        // For objects that are not pages,
        // Add all objects (except Catalog and Pages which we'll handle specially)
        for (object_id, object) in doc.objects.into_iter() {
            match object.type_name().unwrap_or(b"") {
                b"Catalog" | b"Pages" => {
                    // No-op: Skip these, we'll rebuild them
                }
                b"Page" => {
                    // No-op: Pages have been handled separately already
                }
                _ => {
                    all_objects.insert(object_id, object);
                }
            }
        }
    }

    // Insert all collected objects into the merged document
    for (object_id, object) in all_objects {
        merged_doc.objects.insert(object_id, object);
    }

    // Find the Pages object in the merged document
    let catalog = merged_doc.catalog()?;
    let pages_id = catalog
        .get(b"Pages")
        .and_then(|pages_ref| pages_ref.as_reference())
        .map_err(|_| anyhow::anyhow!("Could not find Pages object"))?;

    // Update all page objects to point to the correct parent
    for (page_id, page_obj) in all_pages.iter() {
        if let Ok(dict) = page_obj.as_dict() {
            let mut new_dict = dict.clone();
            new_dict.set("Parent", pages_id);
            merged_doc
                .objects
                .insert(*page_id, Object::Dictionary(new_dict));
        }
    }

    // Update the Pages object with the new page count and kids list
    if let Ok(pages_obj) = merged_doc.get_object_mut(pages_id) {
        if let Ok(pages_dict) = pages_obj.as_dict_mut() {
            // Set the new page count
            pages_dict.set("Count", all_pages.len() as u32);

            // Set the new Kids array with all page references
            let kids: Vec<Object> = all_pages
                .keys()
                .map(|&page_id| Object::Reference(page_id))
                .collect();
            pages_dict.set("Kids", kids);
        }
    }

    // Update max_id and renumber objects to ensure consistency
    // before saving the final merged document
    merged_doc.max_id = merged_doc.objects.len() as u32;
    merged_doc.renumber_objects();

    // Save the merged document
    merged_doc.save(output_path.as_ref())?;
    println!("Time taken to merge final PDF: {:?}", start_time.elapsed());
    Ok(())
}
