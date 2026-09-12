//! Feature under test: an Image XObject's raw pixel bytes are never touched,
//! even when:
//!   (a) those bytes happen to look like alphanumeric text, and
//!   (b) the image dictionary itself carries a `/Metadata` reference to
//!       another object.
//!
//! (b) matters because the referencing dictionary having a `/Metadata` *key*
//! must not be confused with the referenced stream itself being a metadata
//! stream (see CONTEXT.md's "Visual Asset Preservation" / redact_stream_object's
//! doc comment) — only a stream whose own `/Type` is `/Metadata` should be
//! textually redacted.

mod common;

use common::{new_test_doc, run_case};
use lopdf::{Object, Stream, content::Content, dictionary};

/// Build a `size x size` 8bpc grayscale image whose every pixel byte is
/// itself an ASCII alphanumeric character (`0-9A-Za-z`), diagonally shifted
/// row-to-row so the result renders as a visible striped pattern rather than
/// a flat gray square — while every single byte in it is something a naive
/// text-redaction pass could plausibly mistake for redactable content.
fn alphanumeric_striped_pixels(size: usize) -> Vec<u8> {
    let alnum: Vec<u8> = (b'0'..=b'9').chain(b'A'..=b'Z').chain(b'a'..=b'z').collect();
    let mut pixels = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            pixels.push(alnum[(x + y) % alnum.len()]);
        }
    }
    pixels
}

#[test]
fn image_pixel_bytes_survive_redaction_untouched() {
    // A 64x64 grayscale image whose raw bytes are entirely ASCII
    // alphanumeric characters, so an incorrect implementation that redacted
    // image data would be caught here — and large/patterned enough to
    // actually be visible (as diagonal stripes) when the generated PDF is
    // opened in a viewer.
    const SIZE: usize = 64;
    let pixel_bytes = alphanumeric_striped_pixels(SIZE);
    assert_eq!(pixel_bytes.len(), SIZE * SIZE); // one byte per pixel (8bpc grayscale)

    let mut fixture = new_test_doc("BT /F1 12 Tf 100 700 Td (Test: image pixels untouched) Tj ET");

    // A separate (unrelated-content) metadata stream that the image merely
    // points at via /Metadata — this must not cause the image's own stream
    // to be mistaken for metadata.
    let unrelated_metadata_id = fixture.doc.add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<x/>".to_vec(),
    ));

    let image_id = fixture.doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => SIZE as i64,
            "Height" => SIZE as i64,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
            "Metadata" => Object::Reference(unrelated_metadata_id),
        },
        pixel_bytes.clone(),
    ));

    // Reference the image from the page so it's reachable the same way a
    // real PDF viewer would find it, and actually paint it via `Do`.
    let page = fixture
        .doc
        .get_object_mut(fixture.page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    let resources = page.get_mut(b"Resources").unwrap().as_dict_mut().unwrap();
    resources.set(
        "XObject",
        dictionary! { "Im0" => Object::Reference(image_id) },
    );

    let content_id = *fixture.doc.get_page_contents(fixture.page_id).first().unwrap();
    let mut content = Content::decode(
        &fixture
            .doc
            .get_object(content_id)
            .unwrap()
            .as_stream()
            .unwrap()
            .decompressed_content()
            .unwrap(),
    )
    .unwrap();
    // An image XObject paints into the unit square under the current
    // transform, so it must be scaled up via `cm` to be visible — 250x250pt,
    // positioned below the text.
    content
        .operations
        .push(lopdf::content::Operation::new("q", vec![]));
    content.operations.push(lopdf::content::Operation::new(
        "cm",
        vec![250.into(), 0.into(), 0.into(), 250.into(), 100.into(), 400.into()],
    ));
    content.operations.push(lopdf::content::Operation::new(
        "Do",
        vec![Object::Name(b"Im0".to_vec())],
    ));
    content
        .operations
        .push(lopdf::content::Operation::new("Q", vec![]));
    fixture
        .doc
        .get_object_mut(content_id)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_plain_content(content.encode().unwrap());

    let doc = run_case("image_pixel_data_preserved", fixture.doc);

    let image = doc.get_object(image_id).unwrap().as_stream().unwrap();
    assert_eq!(image.content, pixel_bytes, "image pixel bytes must be byte-identical after redaction");
}
