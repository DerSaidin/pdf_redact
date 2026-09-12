//! Feature under test: a `/Metadata` XMP stream attached to an *image*
//! XObject is still redacted like any other metadata stream — being
//! referenced from an image, rather than from the Catalog, must not exempt
//! it. Its identity comes from its own `/Type /Metadata`, not from who
//! points at it.

mod common;

use common::{new_test_doc, run_case};
use lopdf::{Object, Stream, dictionary};

const IMAGE_XMP: &str = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 5.6">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
   tiff:ImageWidth="733"
   tiff:ImageLength="734"
   tiff:Make="Test: image XMP metadata"/>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#;

#[test]
fn redacts_metadata_stream_attached_to_an_image() {
    let mut fixture = new_test_doc("BT /F1 12 Tf 100 700 Td (Test: image XMP metadata) Tj ET");

    let image_metadata_id = fixture.doc.add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        IMAGE_XMP.as_bytes().to_vec(),
    ));

    let image_id = fixture.doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 4,
            "Height" => 4,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
            "Metadata" => Object::Reference(image_metadata_id),
        },
        b"Bc19Zz0Aa5Xy3De7".to_vec(),
    ));

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

    let doc = run_case("image_metadata_xmp", fixture.doc);

    let stream = doc.get_object(image_metadata_id).unwrap().as_stream().unwrap();
    let xml = String::from_utf8(stream.decompressed_content().unwrap()).unwrap();

    // Numeric attribute values redacted...
    assert!(xml.contains(r#"tiff:ImageWidth="000""#));
    assert!(xml.contains(r#"tiff:ImageLength="000""#));
    assert!(xml.contains("Aaaa: aaaaa AAA aaaaaaaa"));
    // ...but structure (element/attribute names, namespace decl) preserved.
    assert!(xml.contains(r#"xmlns:tiff="http://ns.adobe.com/tiff/1.0/""#));
    assert!(xml.contains("tiff:Make="));

    // The image's own pixel data is a separate object and remains untouched.
    let image = doc.get_object(image_id).unwrap().as_stream().unwrap();
    assert_eq!(image.content, b"Bc19Zz0Aa5Xy3De7");
}
