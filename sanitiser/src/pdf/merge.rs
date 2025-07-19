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
pub fn merge_pdf_files<P>(files: &[P], output_path: &P) -> Result<(), Error>
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

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Document;
    use std::fs::File;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn get_test_pdf_path(filename: &str) -> PathBuf {
        // Navigate to the workspace root and then to tests directory
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // Go up from sanitiser/ to workspace root
        path.push("tests");
        path.push(filename);
        path
    }

    #[test]
    fn test_merge_single_file() {
        // Test merging a single file (should just copy it)
        let input = get_test_pdf_path("page-sizes-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = merge_pdf_files(&[input], &output_path);
        assert!(result.is_ok(), "Failed to merge single PDF: {:?}", result);

        // Verify the output file exists and is a valid PDF
        assert!(output_path.exists(), "Output file does not exist");
        let output_file = File::open(&output_path).expect("Failed to open output file");
        let output_doc = Document::load_from(output_file);
        assert!(output_doc.is_ok(), "Output is not a valid PDF");
    }

    #[test]
    fn test_merge_two_files() {
        let input1 = get_test_pdf_path("page-sizes-test.pdf");
        let input2 = get_test_pdf_path("annotations-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = merge_pdf_files(&[input1.clone(), input2.clone()], &output_path);
        assert!(result.is_ok(), "Failed to merge two PDFs: {:?}", result);

        // Verify the output file exists and is a valid PDF
        assert!(output_path.exists(), "Output file does not exist");
        let output_file = File::open(&output_path).expect("Failed to open output file");
        let output_doc = Document::load_from(output_file);
        assert!(output_doc.is_ok(), "Output is not a valid PDF");

        // Verify that the merged document has pages from both inputs
        let merged_doc = output_doc.unwrap();
        let merged_pages = merged_doc.get_pages();

        // Load original documents to count their pages
        let file1 = File::open(&input1).expect("Failed to open first PDF");
        let doc1 = Document::load_from(file1).expect("Failed to load first PDF");
        let file2 = File::open(&input2).expect("Failed to open second PDF");
        let doc2 = Document::load_from(file2).expect("Failed to load second PDF");
        let pages1 = doc1.get_pages();
        let pages2 = doc2.get_pages();

        assert_eq!(
            merged_pages.len(),
            pages1.len() + pages2.len(),
            "Merged PDF should have pages from both input documents"
        );
    }

    #[test]
    fn test_merge_multiple_files() {
        let input1 = get_test_pdf_path("page-sizes-test.pdf");
        let input2 = get_test_pdf_path("annotations-test.pdf");
        let input3 = get_test_pdf_path("image-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = merge_pdf_files(
            &[input1.clone(), input2.clone(), input3.clone()],
            &output_path,
        );
        assert!(
            result.is_ok(),
            "Failed to merge multiple PDFs: {:?}",
            result
        );

        // Verify the output file exists and is a valid PDF
        assert!(output_path.exists(), "Output file does not exist");
        let output_file = File::open(&output_path).expect("Failed to open output file");
        let output_doc = Document::load_from(output_file);
        assert!(output_doc.is_ok(), "Output is not a valid PDF");

        // Verify that the merged document has pages from all inputs
        let merged_doc = output_doc.unwrap();
        let merged_pages = merged_doc.get_pages();

        // Load original documents to count their pages
        let file1 = File::open(&input1).expect("Failed to open first PDF");
        let doc1 = Document::load_from(file1).expect("Failed to load first PDF");
        let file2 = File::open(&input2).expect("Failed to open second PDF");
        let doc2 = Document::load_from(file2).expect("Failed to load second PDF");
        let file3 = File::open(&input3).expect("Failed to open third PDF");
        let doc3 = Document::load_from(file3).expect("Failed to load third PDF");
        let pages1 = doc1.get_pages();
        let pages2 = doc2.get_pages();
        let pages3 = doc3.get_pages();

        assert_eq!(
            merged_pages.len(),
            pages1.len() + pages2.len() + pages3.len(),
            "Merged PDF should have pages from all input documents"
        );
    }

    #[test]
    fn test_merge_empty_input_list() {
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = merge_pdf_files::<PathBuf>(&[], &output_path);
        assert!(result.is_err(), "Should return error for empty input list");

        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("No input files provided"),
            "Error message should mention no input files, got: {}",
            error_message
        );
    }

    #[test]
    fn test_merge_nonexistent_file() {
        let nonexistent = PathBuf::from("/path/that/does/not/exist.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        let result = merge_pdf_files(&[nonexistent], &output_path);
        assert!(result.is_err(), "Should return error for nonexistent file");
    }

    #[test]
    fn test_merge_with_invalid_output_path() {
        let input = get_test_pdf_path("page-sizes-test.pdf");
        let invalid_output = PathBuf::from("/invalid/directory/that/does/not/exist/output.pdf");

        let result = merge_pdf_files(&[input], &invalid_output);
        assert!(
            result.is_err(),
            "Should return error for invalid output path"
        );
    }

    #[test]
    fn test_merged_pdf_structure() {
        // Test that the merged PDF has a proper structure with correct page count
        let input1 = get_test_pdf_path("page-sizes-test.pdf");
        let input2 = get_test_pdf_path("export-test.pdf");
        let output_file = NamedTempFile::new().expect("Failed to create temp file");
        let output_path = output_file.path().to_path_buf();

        merge_pdf_files(&[input1, input2], &output_path.clone()).expect("Failed to merge PDFs");

        let output_file = File::open(&output_path).expect("Failed to open output file");
        let merged_doc = Document::load_from(output_file).expect("Failed to load merged PDF");

        // Check that we have a valid catalog
        let catalog = merged_doc.catalog();
        assert!(catalog.is_ok(), "Merged PDF should have a valid catalog");

        // Check that we have a valid pages object
        let catalog = catalog.unwrap();
        let pages_ref = catalog.get(b"Pages").and_then(|p| p.as_reference());
        assert!(
            pages_ref.is_ok(),
            "Catalog should reference a valid Pages object"
        );

        let pages_id = pages_ref.unwrap();
        let pages_obj = merged_doc.get_object(pages_id);
        assert!(pages_obj.is_ok(), "Should be able to get Pages object");

        // Verify the pages object has the correct structure
        let pages_dict = pages_obj.unwrap().as_dict();
        assert!(pages_dict.is_ok(), "Pages object should be a dictionary");

        let pages_dict = pages_dict.unwrap();

        // Check that Count and Kids are present
        assert!(pages_dict.has(b"Count"), "Pages object should have Count");
        assert!(
            pages_dict.has(b"Kids"),
            "Pages object should have Kids array"
        );

        // Verify the count matches the kids array length
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
}
