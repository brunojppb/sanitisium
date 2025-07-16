use lopdf::Error;
use lopdf::{Document, Object};
use std::path::Path;

/// Merge every file in `inputs` (they were created earlier in 5-page chunks)
/// into `output_path`.  The first file becomes the “base”; all others are
/// appended.
pub fn merge_pdf_files<I, P>(inputs: I, output_path: P) -> Result<(), Error>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{

    let mut iter = inputs.into_iter();

    // -- 1️⃣ load the first chunk; it will donate the Catalog and Page tree ----
    let first_path = iter
        .next()
        .ok_or_else(|| Error::DictKey("missing path".to_owned()))?;
    let mut doc = Document::load(first_path.as_ref())?;
    let pages_root = doc
        .get_pages()
        .iter()
        .next()
        .map(|(_, id)| *id)
        .ok_or_else(|| Error::DictKey("missing pages".to_owned()))?;

    // Remember all page IDs we will finally list in /Kids
    let mut kids: Vec<Object> = Vec::new();
    // and always keep track of the next unused object number
    let mut max_id = doc.max_id + 1;

    // Insert the pages that are already in the first file
    for (_, page_id) in doc.get_pages() {
        kids.push(Object::Reference(page_id));
    }

    // -- 2️⃣ fold every other chunk into `doc` one by one ----------------------
    for path in iter {
      println!("Merging path={:?}", path.as_ref());
        let mut other = Document::load(path.as_ref())?;

        // shift object numbers so there are no clashes
        other.renumber_objects_with(max_id);
        max_id = other.max_id + 1;

        
        

        // move every indirect object
        // doc.objects.extend(other.objects);

        // for every Page in that file …
        for (_idx, page_id) in other.get_pages() {
          doc.objects.append(&mut other.objects);
            //  a) tell it that its new parent is *our* Pages root
            if let Ok(page_dict) = doc.get_object_mut(page_id)?.as_dict_mut() {
                page_dict.set("Parent", Object::Reference(pages_root));
            }
            //  b) add it to the new /Kids array
            kids.push(Object::Reference(page_id));
        }
    }

    // -- 3️⃣ patch the Pages node so it knows about the new /Kids and /Count ---
    if let Ok(dict) = doc.get_object_mut(pages_root)?.as_dict_mut() {
        dict.set("Kids", Object::Array(kids));
        dict.set("Count", dict.len() as i64);
    }

    // -- 4️⃣ final book-keeping -------------------------------------------------
    doc.renumber_objects(); // make the ID space dense again (optional)
    doc.adjust_zero_pages(); // fix bookmarks whose /Page = 0
    doc.compress(); // Flate-compress every stream (keeps size down)
    doc.save(output_path)?;

    Ok(())
}
