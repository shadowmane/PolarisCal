//! Appending one PDF's pages to the end of another (test data after a certificate).
//! Adapted from lopdf's merge example: each page keeps the attributes it inherited from
//! its own document, and the pages keep their order.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use lopdf::{Dictionary, Document, Error, Object, ObjectId};

fn inherited(doc: &Document, page_id: ObjectId, key: &[u8]) -> lopdf::Result<Option<Object>> {
    let mut current = page_id;
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current) {
            return Err(Error::ReferenceCycle(current));
        }
        let node = doc.get_dictionary(current)?;
        if let Ok(value) = node.get(key) {
            match doc.dereference(value) {
                Ok((_, Object::Null)) | Err(Error::ObjectNotFound(_)) => {}
                Ok(_) => return Ok(Some(value.clone())),
                Err(error) => return Err(error),
            }
        }
        match node.get(b"Parent") {
            Err(_) | Ok(Object::Null) => return Ok(None),
            Ok(parent) => current = parent.as_reference()?,
        }
    }
}

/// The page with everything it inherits written onto it, so it can move to a new parent.
fn standalone_page(doc: &Document, page_id: ObjectId) -> lopdf::Result<Dictionary> {
    let mut page = doc.get_dictionary(page_id)?.clone();
    let media_box = inherited(doc, page_id, b"MediaBox")?
        .unwrap_or_else(|| vec![0.into(), 0.into(), 612.into(), 792.into()].into());
    let crop_box = inherited(doc, page_id, b"CropBox")?.unwrap_or_else(|| media_box.clone());
    let resources = inherited(doc, page_id, b"Resources")?
        .unwrap_or_else(|| Object::Dictionary(Dictionary::new()));
    let rotation = inherited(doc, page_id, b"Rotate")?.unwrap_or(Object::Integer(0));
    page.set("MediaBox", media_box);
    page.set("CropBox", crop_box);
    page.set("Resources", resources);
    page.set("Rotate", rotation);
    Ok(page)
}

/// One document with the pages of `documents`, in order.
pub fn merge(documents: Vec<Document>) -> Result<Document, String> {
    let mut max_id = 1;
    let mut pages: Vec<(ObjectId, Object)> = Vec::new();
    let mut objects = BTreeMap::new();
    for mut doc in documents {
        for page_id in doc.get_pages().into_values() {
            let page = standalone_page(&doc, page_id).map_err(|e| e.to_string())?;
            doc.objects.insert(page_id, Object::Dictionary(page));
        }
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;
        for page_id in doc.get_pages().into_values() {
            let page = doc
                .get_object(page_id)
                .map_err(|e| e.to_string())?
                .to_owned();
            pages.push((page_id, page));
        }
        objects.extend(doc.objects);
    }

    let mut merged = Document::with_version("1.7");
    let mut catalog: Option<(ObjectId, Object)> = None;
    let mut root_pages: Option<ObjectId> = None;
    for (id, object) in objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                if catalog.is_none() {
                    catalog = Some((id, object));
                }
            }
            b"Pages" => {
                // The first document's page tree root becomes the merged one; other
                // intermediate nodes are dropped since every page now stands alone.
                if root_pages.is_none() {
                    root_pages = Some(id);
                }
            }
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                merged.objects.insert(id, object);
            }
        }
    }
    let pages_id = root_pages.ok_or("a document has no pages")?;
    let (catalog_id, catalog) = catalog.ok_or("a document has no catalog")?;

    for (id, page) in &pages {
        if let Ok(dict) = page.as_dict() {
            let mut dict = dict.clone();
            dict.set("Parent", pages_id);
            merged.objects.insert(*id, Object::Dictionary(dict));
        }
    }
    let mut tree = Dictionary::new();
    tree.set("Type", "Pages");
    tree.set("Count", pages.len() as u32);
    tree.set(
        "Kids",
        pages
            .iter()
            .map(|(id, _)| Object::Reference(*id))
            .collect::<Vec<_>>(),
    );
    merged.objects.insert(pages_id, Object::Dictionary(tree));

    let mut catalog = catalog.as_dict().map_err(|e| e.to_string())?.clone();
    catalog.set("Pages", pages_id);
    catalog.remove(b"Outlines");
    catalog.remove(b"PageMode");
    merged
        .objects
        .insert(catalog_id, Object::Dictionary(catalog));
    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.keys().map(|(n, _)| *n).max().unwrap_or(0);
    merged.renumber_objects();
    merged.compress();
    Ok(merged)
}

/// Appends the pages of the PDF at `extra` to the PDF at `main`, rewriting `main`.
pub fn append_pdf(main: &Path, extra: &Path) -> Result<(), String> {
    let first =
        Document::load(main).map_err(|e| format!("couldn't reread the certificate: {e}"))?;
    let second =
        Document::load(extra).map_err(|e| format!("couldn't read the test data PDF: {e}"))?;
    if second.is_encrypted() {
        return Err("the test data PDF is password-protected, so it can't be added.".into());
    }
    let mut merged = merge(vec![first, second])?;
    let temp = main.with_extension("pdf.partial");
    merged
        .save(&temp)
        .map_err(|e| format!("couldn't write the combined PDF: {e}"))?;
    std::fs::rename(&temp, main).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("couldn't replace the certificate PDF: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{Stream, dictionary};

    fn document(texts: &[&str]) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let resources_id =
            doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
        let mut kids = Vec::new();
        for text in texts {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![72.into(), 700.into()]),
                    Operation::new("Tj", vec![Object::string_literal(*text)]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            kids.push(Object::Reference(doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
            })));
        }
        // Resources and size are inherited from the page tree, as many PDFs do.
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Count" => kids.len() as u32, "Kids" => kids,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    #[test]
    fn pages_are_appended_in_order_with_their_resources() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("certificate.pdf");
        let extra = dir.path().join("data.pdf");
        document(&["Certificate"]).save(&main).unwrap();
        document(&["Data 1", "Data 2"]).save(&extra).unwrap();

        append_pdf(&main, &extra).unwrap();

        let merged = Document::load(&main).unwrap();
        assert_eq!(merged.get_pages().len(), 3);
        let text: Vec<String> = (1..=3)
            .map(|n| merged.extract_text(&[n]).unwrap().trim().to_string())
            .collect();
        assert_eq!(text, ["Certificate", "Data 1", "Data 2"]);
        assert!(!dir.path().join("certificate.pdf.partial").exists());
    }

    #[test]
    fn a_file_that_isnt_a_pdf_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("certificate.pdf");
        let extra = dir.path().join("data.pdf");
        document(&["Certificate"]).save(&main).unwrap();
        std::fs::write(&extra, b"not a pdf").unwrap();
        let err = append_pdf(&main, &extra).unwrap_err();
        assert!(err.contains("test data"), "{err}");
        assert_eq!(
            Document::load(&main).unwrap().get_pages().len(),
            1,
            "the certificate is untouched"
        );
    }
}
