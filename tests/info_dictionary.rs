//! Feature under test: string values in the document `/Info` dictionary
//! (Title, Author, Creator, CreationDate) are redacted, independent of the
//! page content stream.

mod common;

use common::{new_test_doc, pdf_string, run_case};
use lopdf::{Object, StringFormat, dictionary};

#[test]
fn redacts_info_dictionary_strings() {
    let page_content = "BT /F1 12 Tf 100 700 Td (Test: Info dictionary strings are redacted) Tj ET";

    let mut fixture = new_test_doc(page_content);

    let info_id = fixture.doc.add_object(dictionary! {
        "Title" => pdf_string("Stuff 2014"),
        "Author" => pdf_string("Prepared by: Foo Finance"),
        "Creator" => pdf_string("My Report Generator 9.1 x86-64"),
        "CreationDate" => pdf_string("D:20140506153509+10'00'"),
    });
    fixture.doc.trailer.set("Info", Object::Reference(info_id));

    let doc = run_case("info_dictionary", fixture.doc);

    let info = doc.get_object(info_id).unwrap().as_dict().unwrap();
    let get_str = |key: &[u8]| -> String {
        match info.get(key).unwrap() {
            Object::String(bytes, StringFormat::Literal) => String::from_utf8(bytes.clone()).unwrap(),
            other => panic!("expected literal string, got {other:?}"),
        }
    };

    assert_eq!(get_str(b"Title"), "Aaaaa 0000");
    assert_eq!(get_str(b"Author"), "Aaaaaaaa aa: Aaa Aaaaaaa");
    assert_eq!(get_str(b"Creator"), "Aa Aaaaaa Aaaaaaaaa 0.0 a00-00");
    assert_eq!(get_str(b"CreationDate"), "A:00000000000000+00'00'");
}
