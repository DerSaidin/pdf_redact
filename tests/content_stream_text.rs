//! Feature under test: text-operator strings (`Tj`) in a page content stream
//! are redacted character-for-character, while punctuation, whitespace and
//! every other content-stream byte is left untouched.

mod common;

use common::{new_test_doc, page_content_bytes, run_case};

#[test]
fn redacts_tj_text_preserving_punctuation_and_length() {
    let text = "Test: content-stream Tj text is redacted 1:1 - Hello World 123!";
    let page_content = format!("BT /F1 12 Tf 100 700 Td ({text}) Tj ET");

    let fixture = new_test_doc(&page_content);
    let page_id = fixture.page_id;
    let doc = run_case("content_stream_text", fixture.doc);

    let redacted = page_content_bytes(&doc, page_id);
    let redacted = String::from_utf8(redacted).expect("content stream is valid UTF-8");

    // Alphanumerics inside the Tj string became placeholders, 1:1 in length...
    let expected_text = "Aaaa: aaaaaaa-aaaaaa Aa aaaa aa aaaaaaaa 0:0 - Aaaaa Aaaaa 000!";
    assert_eq!(expected_text.len(), text.len());
    assert!(redacted.contains(&format!("({expected_text}) Tj")));
    // ...while the surrounding operator syntax (font, position) is untouched.
    // (lopdf's content-stream encoder reformats whitespace/newlines between
    // operators, so we check for the operators and operands rather than the
    // exact original byte layout.)
    assert!(redacted.contains("BT"));
    assert!(redacted.contains("/F1 12 Tf"));
    assert!(redacted.contains("100 700 Td"));
    assert!(redacted.contains("ET"));
}
