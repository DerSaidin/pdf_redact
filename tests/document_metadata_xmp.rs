//! Feature under test: the document-level XMP `/Metadata` stream (referenced
//! from the Catalog) has its text nodes and attribute *values* redacted,
//! while element names, attribute names, and `xmlns`/`xmlns:*` namespace
//! declarations are left untouched so the packet stays well-formed.

mod common;

use common::{new_test_doc, run_case};
use lopdf::{Object, Stream, dictionary};

const XMP: &str = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 9.0-c001 79.14ecb42">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:xmpMM="http://ns.adobe.com/xap/1.0/mm/"
    xmlns:stEvt="http://ns.adobe.com/xap/1.0/sType/ResourceEvent#"
   xmp:CreatorTool="Adobe Photoshop CS6 (Macintosh)"
   xmpMM:DocumentID="uuid:0D53A7FF7158DD11B20FE126ED3FD217">
   <dc:title>
    <rdf:Alt>
     <rdf:li xml:lang="x-default">Test: XMP metadata 42</rdf:li>
    </rdf:Alt>
   </dc:title>
   <xmpMM:History>
    <rdf:Seq>
     <rdf:li
      stEvt:action="created"
      stEvt:softwareAgent="Adobe Photoshop CS6 (Macintosh)"/>
    </rdf:Seq>
   </xmpMM:History>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#;

#[test]
fn redacts_document_xmp_metadata_preserving_xml_structure() {
    let page_content = "BT /F1 12 Tf 100 700 Td (Test: document XMP metadata) Tj ET";
    let mut fixture = new_test_doc(page_content);

    let metadata_id = fixture.doc.add_object(Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
        },
        XMP.as_bytes().to_vec(),
    ));

    let catalog = fixture
        .doc
        .get_object_mut(fixture.catalog_id)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    catalog.set("Metadata", Object::Reference(metadata_id));

    let doc = run_case("document_metadata_xmp", fixture.doc);

    let stream = doc.get_object(metadata_id).unwrap().as_stream().unwrap();
    let xml = String::from_utf8(stream.decompressed_content().unwrap()).unwrap();

    // Text node content redacted.
    assert!(xml.contains("Aaaa: AAA aaaaaaaa 00"));
    // Attribute *values* redacted...
    assert!(xml.contains(r#"xmp:CreatorTool="Aaaaa Aaaaaaaaa AA0 (Aaaaaaaaa)""#));
    assert!(xml.contains(r#"stEvt:action="aaaaaaa""#));
    // ...but element names, attribute names, and namespace declarations are
    // preserved verbatim.
    assert!(xml.contains(r#"xmlns:x="adobe:ns:meta/""#));
    assert!(xml.contains(r#"xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#""#));
    assert!(xml.contains(r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#));
    assert!(xml.contains("<dc:title>"));
    assert!(xml.contains("<xmpMM:History>"));
    assert!(xml.contains("<rdf:Seq>"));
}
