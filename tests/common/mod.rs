//! Shared helpers for building minimal PDF fixtures and running them
//! through `pdf_redact::redact_document`.
//!
//! Every test case's source and redacted PDF are always written to
//! `target/test-artifacts/` (see [`run_case`]) so they can be opened by hand
//! after `cargo test` to inspect exactly what was produced.
//!
//! Each `tests/*.rs` file is compiled as its own separate binary, and no
//! single one uses every helper here — hence the blanket `dead_code` allow.
#![allow(dead_code)]

use std::path::PathBuf;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use pdf_redact::redact_document;

pub fn artifacts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-artifacts")
}

/// Build a PDF `Object::String` in literal format (i.e. a parenthesized PDF
/// string like `(Hello)`, as opposed to `Object::Name` which `&str`'s
/// built-in `Into<Object>` produces).
pub fn pdf_string(s: &str) -> Object {
    Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
}

/// A minimal single-page PDF: Catalog -> Pages -> Page -> Contents, with a
/// Helvetica font, ready for test-specific objects (Info, XMP metadata,
/// images, ...) to be layered on before being run through the redactor.
///
/// This mirrors the object graph of the hand-authored `test.pdf` fixture
/// that motivated these tests.
pub struct TestDoc {
    pub doc: Document,
    pub catalog_id: ObjectId,
    pub page_id: ObjectId,
}

/// `page_content` is the raw (uncompressed) page content-stream text, e.g.
/// `"BT /F1 12 Tf 100 700 Td (Hello) Tj ET"`.
pub fn new_test_doc(page_content: &str) -> TestDoc {
    let mut doc = Document::with_version("1.5");

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let content_id = doc.add_object(Stream::new(Dictionary::new(), page_content.as_bytes().to_vec()));

    let pages_id = doc.new_object_id();

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        },
    });

    doc.set_object(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        },
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    });

    doc.trailer.set("Root", Object::Reference(catalog_id));

    TestDoc { doc, catalog_id, page_id }
}

/// Always write `doc` to `target/test-artifacts/<name>.pdf`, redact it via
/// [`pdf_redact::redact_document`], write the result to
/// `target/test-artifacts/<name>.redacted.pdf`, and return the redacted
/// `Document` for assertions.
pub fn run_case(name: &str, mut doc: Document) -> Document {
    let dir = artifacts_dir();
    std::fs::create_dir_all(&dir).expect("create target/test-artifacts");

    doc.save(dir.join(format!("{name}.pdf")))
        .expect("write source test PDF");

    redact_document(&mut doc, false);

    doc.save(dir.join(format!("{name}.redacted.pdf")))
        .expect("write redacted test PDF");

    doc
}

/// Extract the plain-text bytes of a page's content stream (decompressed).
pub fn page_content_bytes(doc: &Document, page_id: ObjectId) -> Vec<u8> {
    let content_ids = doc.get_page_contents(page_id);
    let mut out = Vec::new();
    for id in content_ids {
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        out.extend(stream.decompressed_content().unwrap_or_default());
    }
    out
}
