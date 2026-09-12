use std::path::PathBuf;

use clap::Parser;
use lopdf::{Document, Object, ObjectId, Stream, StringFormat, content::Content};

/// PDF Redactor — replaces all alphanumeric content in a PDF with neutral
/// placeholder characters while preserving structure, styles and layout.
///
/// Each character is replaced 1:1:
///   a-z → 'a', A-Z → 'A', 0-9 → '0'
/// Non-ASCII alphanumeric characters follow the same case/digit rules.
/// Whitespace, punctuation, kerning offsets and every PDF structural byte are
/// left completely untouched.
#[derive(Parser, Debug)]
#[command(name = "pdf-redact", version, about)]
struct Args {
    /// Input PDF file
    input: PathBuf,

    /// Output file (default: <stem>.redacted.pdf next to the input file)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Write content streams without FlateDecode compression (useful for
    /// inspecting raw stream bytes when creating test fixtures)
    #[arg(long)]
    uncompressed: bool,
}

fn main() {
    let args = Args::parse();

    let output = match &args.output {
        Some(p) => p.clone(),
        None => {
            let stem = args
                .input
                .file_stem()
                .expect("input has no file stem")
                .to_string_lossy();
            let mut p = args.input.clone();
            p.set_file_name(format!("{stem}.redacted.pdf"));
            p
        }
    };

    // -----------------------------------------------------------------------
    // Load — attempt blank-password decryption for encrypted PDFs.
    // -----------------------------------------------------------------------
    let mut doc = match Document::load(&args.input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "error: failed to load '{}': {e}",
                args.input.display()
            );
            std::process::exit(1);
        }
    };

    if doc.is_encrypted() {
        match doc.decrypt("") {
            Ok(()) => {}
            Err(e) => {
                eprintln!(
                    "error: PDF is encrypted and could not be decrypted with an empty password: {e}"
                );
                std::process::exit(1);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Collect all object IDs upfront to avoid borrow conflicts while mutating.
    // -----------------------------------------------------------------------
    let all_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    // -----------------------------------------------------------------------
    // Pass 1 — Redact dictionary string values (Info, Outlines, Annotations,
    // AcroForm fields, and any other dictionary that carries text strings).
    // Also redact /ToUnicode CMap streams and /Metadata XMP streams while
    // visiting stream objects.
    // -----------------------------------------------------------------------
    for id in &all_ids {
        // We take ownership of the object temporarily, redact, and put it back.
        let Some(obj) = doc.objects.remove(id) else {
            continue;
        };
        let obj = redact_object(obj, args.uncompressed);
        doc.objects.insert(*id, obj);
    }

    // -----------------------------------------------------------------------
    // Pass 2 — Redact text operator strings in page/Form-XObject content
    // streams.  We operate on content streams separately because lopdf
    // provides the Content parser for them.
    // -----------------------------------------------------------------------
    let page_ids: Vec<ObjectId> = doc.page_iter().collect();
    for page_id in page_ids {
        let content_ids = doc.get_page_contents(page_id);
        for cid in content_ids {
            redact_content_stream(&mut doc, cid, args.uncompressed);
        }
    }

    // Form XObjects may live anywhere in the object tree — collect them.
    let form_xobject_ids: Vec<ObjectId> = all_ids
        .iter()
        .filter(|id| {
            doc.objects
                .get(id)
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| s.dict.get(b"Subtype").ok())
                .and_then(|v| v.as_name().ok())
                .map(|n| n == b"Form")
                .unwrap_or(false)
        })
        .copied()
        .collect();

    for id in form_xobject_ids {
        redact_content_stream(&mut doc, id, args.uncompressed);
    }

    // -----------------------------------------------------------------------
    // Save
    // -----------------------------------------------------------------------
    match doc.save(&output) {
        Ok(_) => println!("{}", output.display()),
        Err(e) => {
            eprintln!("error: failed to write '{}': {e}", output.display());
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Character-level redaction
// ---------------------------------------------------------------------------

/// Redact a single Unicode scalar: letters → 'a'/'A', digits → '0'.
#[inline]
fn redact_char(c: char) -> char {
    if c.is_ascii_alphabetic() {
        if c.is_uppercase() { 'A' } else { 'a' }
    } else if c.is_ascii_digit() {
        '0'
    } else if c.is_alphabetic() {
        // Non-ASCII letters: preserve case where meaningful
        if c.is_uppercase() { 'A' } else { 'a' }
    } else if c.is_numeric() {
        '0'
    } else {
        c
    }
}

/// Redact a raw PDF string byte-by-byte (for Literal strings using a simple
/// single-byte encoding such as PDFDocEncoding / WinAnsi / MacRoman).
///
/// We treat each byte as ISO-Latin-1 (one Unicode code-point per byte) so
/// that the output keeps the same length and every printable ASCII alphanumeric
/// is replaced.  Non-printable control bytes and bytes above 0x7F that do not
/// map to a letter or digit in Latin-1 are left unchanged.
fn redact_bytes_literal(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|&b| {
            let c = b as char; // ISO-Latin-1 code point
            redact_char(c) as u8
        })
        .collect()
}

/// Redact a hex-encoded PDF string (used for composite/CID fonts).
///
/// The string arrives already decoded as raw bytes by lopdf.  For 2-byte CID
/// fonts each pair of bytes encodes one glyph code; we must not corrupt that
/// alignment.  We therefore redact on a per-byte basis, replacing bytes that
/// correspond to ASCII alphanumeric glyph codes while leaving the high byte of
/// 2-byte sequences alone.  This is conservative: we only redact the low byte
/// when it falls in [0x30-0x39, 0x41-0x5A, 0x61-0x7A] and the surrounding
/// context (byte count) suggests a 1-byte or 2-byte sequence.
///
/// In practice most hex strings in modern PDFs store UTF-16BE code points, so
/// we attempt to interpret the bytes as UTF-16BE first and fall back to byte-
/// level redaction.
fn redact_bytes_hex(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() % 2 == 0 {
        // Attempt UTF-16BE interpretation
        let code_units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        if let Ok(s) = String::from_utf16(&code_units) {
            // Successfully decoded — redact and re-encode as UTF-16BE
            let redacted: String = s.chars().map(redact_char).collect();
            let mut out = Vec::with_capacity(bytes.len());
            for cu in redacted.encode_utf16() {
                out.extend_from_slice(&cu.to_be_bytes());
            }
            if out.len() == bytes.len() {
                return out;
            }
        }
    }
    // Fall back: treat as a sequence of 1-byte glyph codes
    redact_bytes_literal(bytes)
}

/// Redact a PDF string `Object::String(bytes, format)` in place.
fn redact_string_object(bytes: &[u8], format: &StringFormat) -> Vec<u8> {
    match format {
        StringFormat::Literal => redact_bytes_literal(bytes),
        StringFormat::Hexadecimal => redact_bytes_hex(bytes),
    }
}

// ---------------------------------------------------------------------------
// Object-level redaction (dictionaries, strings, stream special cases)
// ---------------------------------------------------------------------------

/// Walk an `Object` tree and redact every `String` value.
///
/// Stream objects with `/Subtype /ToUnicode` (CMap) or that are `/Metadata`
/// (XMP) receive special treatment — their raw bytes are textually redacted
/// rather than being parsed as PDF content operators.
fn redact_object(obj: Object, uncompressed: bool) -> Object {
    match obj {
        Object::String(bytes, format) => {
            let redacted = redact_string_object(&bytes, &format);
            Object::String(redacted, format)
        }
        Object::Array(items) => {
            Object::Array(items.into_iter().map(|o| redact_object(o, uncompressed)).collect())
        }
        Object::Dictionary(mut dict) => {
            for (_, v) in dict.iter_mut() {
                // Redact values in-place by swapping
                let owned = std::mem::replace(v, Object::Null);
                *v = redact_object(owned, uncompressed);
            }
            Object::Dictionary(dict)
        }
        Object::Stream(stream) => redact_stream_object(stream, uncompressed),
        // References, names, numbers, booleans, null — untouched
        other => other,
    }
}

/// Redact a stream object.  Content streams (page content, Form XObjects) are
/// handled in Pass 2 via the Content parser — here we handle only:
///   • `/ToUnicode` CMap streams
///   • `/Metadata` XMP streams
/// All other streams (fonts, images, colour spaces, …) are left untouched.
fn redact_stream_object(mut stream: Stream, uncompressed: bool) -> Object {
    // Identify the stream type from its dictionary.
    let subtype = stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n.to_vec());

    let is_to_unicode = stream.dict.has(b"ToUnicode")
        || subtype.as_deref() == Some(b"ToUnicode" as &[u8]);

    // Detect via key name whether this stream IS a ToUnicode CMap (lopdf
    // sometimes surfaces ToUnicode as the stream itself when the font dict
    // holds a direct stream reference).
    //
    // A more reliable heuristic: check whether the decompressed content starts
    // with "/CIDInit" or "begincmap" — the standard CMap prologue.
    let raw = stream.decompressed_content().unwrap_or_default();

    let is_cmap = raw.windows(9).any(|w| w == b"begincmap");
    // A metadata stream identifies *itself* via `/Type /Metadata` (and usually
    // `/Subtype /XML`).  We must NOT key off a `/Metadata` entry in the dict:
    // that entry appears on the *referencing* object (page, catalog, image
    // XObject, …) pointing at the metadata stream — so matching it would treat
    // e.g. an image that carries `/Metadata N 0 R` as XMP and corrupt its pixels.
    let type_is_metadata = stream
        .dict
        .get(b"Type")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n == b"Metadata")
        .unwrap_or(false);
    let is_metadata = subtype.as_deref() == Some(b"XML" as &[u8]) || type_is_metadata;

    if is_cmap || is_to_unicode {
        let redacted = redact_cmap_bytes(&raw);
        stream.set_plain_content(redacted);
        if !uncompressed {
            let _ = stream.compress();
        }
    } else if is_metadata {
        let redacted = redact_xmp_bytes(&raw);
        stream.set_plain_content(redacted);
        if !uncompressed {
            let _ = stream.compress();
        }
    }
    // else: leave the stream content unchanged (images, font programs, etc.)

    // Still redact any string values stored in the stream dictionary itself
    // (rare, but theoretically possible).
    for (_, v) in stream.dict.iter_mut() {
        let owned = std::mem::replace(v, Object::Null);
        *v = redact_object(owned, uncompressed);
    }

    Object::Stream(stream)
}

// ---------------------------------------------------------------------------
// CMap (ToUnicode) redaction
// ---------------------------------------------------------------------------

/// Redact a ToUnicode CMap stream textually.
///
/// The CMap format uses lines like:
///   `<00> <0041>`          (begincmap bfchar mapping: glyph 0x00 → U+0041 'A')
///   `<0041> <005A>`        (bfrange: glyphs 0x41-0x5A → U+0041-U+005A)
///
/// We rewrite only the *destination* Unicode code points in bfchar and bfrange
/// entries that map to alphanumeric characters, replacing them with their
/// redacted equivalents ('A', 'a', '0').
fn redact_cmap_bytes(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(text.len());

    let mut in_bfchar = false;
    let mut in_bfrange = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "beginbfchar" {
            in_bfchar = true;
            out.push_str(line);
        } else if trimmed == "endbfchar" {
            in_bfchar = false;
            out.push_str(line);
        } else if trimmed == "beginbfrange" {
            in_bfrange = true;
            out.push_str(line);
        } else if trimmed == "endbfrange" {
            in_bfrange = false;
            out.push_str(line);
        } else if in_bfchar {
            out.push_str(&redact_cmap_bfchar_line(line));
        } else if in_bfrange {
            out.push_str(&redact_cmap_bfrange_line(line));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }

    out.into_bytes()
}

/// Parse a `<src> <dst>` bfchar line and redact the destination if it encodes
/// an alphanumeric Unicode code point.
fn redact_cmap_bfchar_line(line: &str) -> String {
    // Collect all `<hex>` tokens
    let tokens = collect_hex_tokens(line);
    if tokens.len() < 2 {
        return line.to_string();
    }
    // The destination is the last token
    let dst_str = &tokens[tokens.len() - 1];
    let redacted = redact_hex_unicode_token(dst_str);
    // Rebuild the line by replacing only the last token
    replace_last_hex_token(line, dst_str, &redacted)
}

/// Parse a `<lo> <hi> <dst>` bfrange line and redact the destination.
fn redact_cmap_bfrange_line(line: &str) -> String {
    let tokens = collect_hex_tokens(line);
    if tokens.len() < 3 {
        return line.to_string();
    }
    let dst_str = &tokens[tokens.len() - 1];
    let redacted = redact_hex_unicode_token(dst_str);
    replace_last_hex_token(line, dst_str, &redacted)
}

/// Collect all `<…>` hex tokens from a CMap line.
fn collect_hex_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('>') {
            tokens.push(rest[..end].to_string());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    tokens
}

/// Replace the last `<old>` token in `line` with `<new_tok>`.
fn replace_last_hex_token(line: &str, old: &str, new_tok: &str) -> String {
    let needle = format!("<{old}>");
    let replacement = format!("<{new_tok}>");
    // Replace only the last occurrence
    if let Some(pos) = line.rfind(&needle) {
        let mut s = line.to_string();
        s.replace_range(pos..pos + needle.len(), &replacement);
        s
    } else {
        line.to_string()
    }
}

/// Given a hex-encoded Unicode scalar (e.g. "0041" = U+0041 = 'A'),
/// return the hex representation of its redacted form.
fn redact_hex_unicode_token(hex: &str) -> String {
    // Decode hex bytes → try to read as UTF-16BE codepoint(s)
    let bytes: Vec<u8> = hex
        .as_bytes()
        .chunks(2)
        .filter_map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap_or(""), 16).ok())
        .collect();

    // Try to interpret as UTF-16BE
    if bytes.len() % 2 == 0 {
        let code_units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        if let Ok(s) = String::from_utf16(&code_units) {
            let redacted: String = s.chars().map(redact_char).collect();
            // Re-encode as UTF-16BE hex
            let encoded: Vec<u8> = redacted
                .encode_utf16()
                .flat_map(|cu| cu.to_be_bytes())
                .collect();
            return encoded.iter().map(|b| format!("{b:02X}")).collect::<String>();
        }
    }

    // Single byte (Simple font CMap)
    if bytes.len() == 1 {
        let c = bytes[0] as char;
        let r = redact_char(c) as u8;
        return format!("{r:02X}");
    }

    // Fallback: return unchanged
    hex.to_uppercase()
}

// ---------------------------------------------------------------------------
// XMP / Metadata redaction
// ---------------------------------------------------------------------------

/// Redact alphanumeric characters inside XMP XML text nodes and attribute
/// values, while leaving XML structure (element/attribute names and namespace
/// declarations) untouched.
///
/// XMP stores most of its payload — `xmp:CreatorTool`, `xmp:CreateDate`,
/// `xmpMM:DocumentID`, `xmpMM:History` events, etc. — in element *attributes*
/// rather than text nodes, so those must be redacted too.  Namespace
/// declarations (`xmlns` / `xmlns:*`) are preserved verbatim because they are
/// structural and required for the packet to remain well-formed RDF.
fn redact_xmp_bytes(raw: &[u8]) -> Vec<u8> {
    use quick_xml::events::{BytesCData, BytesText, Event};
    use quick_xml::{Reader, Writer};
    use std::io::Cursor;

    // Use a buffered reader; read_event_into is the API for Cursor-backed readers.
    let mut reader = Reader::from_reader(Cursor::new(raw));
    reader.config_mut().trim_text(false);

    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => {
                // In quick-xml 0.42, BytesText derefs to str.
                let text = e.as_ref().to_owned();
                let redacted: String = text.chars().map(redact_char).collect();
                // BytesText::new expects escaped &str content
                let owned_event = BytesText::new(&redacted).into_owned();
                let _ = writer.write_event(Event::Text(owned_event));
            }
            Ok(Event::CData(e)) => {
                // BytesCData::into_inner returns Cow<str> in quick-xml 0.42
                let cow = e.into_inner();
                let redacted: String = cow.chars().map(redact_char).collect();
                let owned_event = BytesCData::new(redacted.as_str()).into_owned();
                let _ = writer.write_event(Event::CData(owned_event));
            }
            Ok(Event::Start(e)) => {
                let event = match redact_element_attributes(&e) {
                    Some(elem) => Event::Start(elem),
                    None => Event::Start(e.into_owned()),
                };
                let _ = writer.write_event(event);
            }
            Ok(Event::Empty(e)) => {
                let event = match redact_element_attributes(&e) {
                    Some(elem) => Event::Empty(elem),
                    None => Event::Empty(e.into_owned()),
                };
                let _ = writer.write_event(event);
            }
            Ok(Event::Eof) => break,
            Ok(event) => {
                let _ = writer.write_event(event.into_owned());
            }
            Err(_) => {
                // If XML parsing fails, fall back to a plain textual
                // redaction of the raw bytes.
                return redact_bytes_literal(raw);
            }
        }
        buf.clear();
    }

    writer.into_inner().into_inner()
}

/// Rebuild an XML start/empty element, redacting every attribute *value* while
/// keeping element names, attribute names and namespace declarations intact.
///
/// Returns `None` if the element or any attribute cannot be decoded, in which
/// case the caller emits the element unchanged rather than risk corrupting it.
fn redact_element_attributes(
    e: &quick_xml::events::BytesStart,
) -> Option<quick_xml::events::BytesStart<'static>> {
    use quick_xml::events::BytesStart;

    let name = e.name().into_inner().to_owned();
    let mut out = BytesStart::new(name);

    for attr in e.attributes() {
        let attr = attr.ok()?;
        let key = attr.key.into_inner().to_owned();
        #[allow(deprecated)]
        let value = attr.unescape_value().ok()?;

        // Namespace declarations are structure, not content — preserve them.
        let new_value: String = if key == "xmlns" || key.starts_with("xmlns:") {
            value.into_owned()
        } else {
            value.chars().map(redact_char).collect()
        };

        out.push_attribute((key.as_str(), new_value.as_str()));
    }

    Some(out)
}

// ---------------------------------------------------------------------------
// Content-stream redaction (Pass 2)
// ---------------------------------------------------------------------------

/// Redact all text-operator string operands in a single content stream.
///
/// Text operators that carry string operands:
///   `Tj`  — show string
///   `TJ`  — show array of strings/numbers
///   `'`   — move to next line then show string
///   `"`   — set word/char spacing, move, show string
fn redact_content_stream(doc: &mut Document, stream_id: ObjectId, uncompressed: bool) {
    // Extract raw object; skip if not a stream.
    let Some(obj) = doc.objects.remove(&stream_id) else {
        return;
    };
    let Ok(mut stream) = obj.try_into_stream() else {
        // Not a stream — put it back unchanged.
        return;
    };

    let raw = match stream.decompressed_content() {
        Ok(b) => b,
        Err(_) => {
            // Cannot decompress; put back unchanged.
            doc.objects.insert(stream_id, Object::Stream(stream));
            return;
        }
    };

    let mut content = match Content::decode(&raw) {
        Ok(c) => c,
        Err(_) => {
            doc.objects.insert(stream_id, Object::Stream(stream));
            return;
        }
    };

    for op in &mut content.operations {
        match op.operator.as_str() {
            // Tj, ', " — first or third operand is a string
            "Tj" | "'" => {
                if let Some(obj) = op.operands.first_mut() {
                    redact_operand_string(obj);
                }
            }
            "\"" => {
                // " takes: aw ac string
                if let Some(obj) = op.operands.get_mut(2) {
                    redact_operand_string(obj);
                }
            }
            // TJ — array of strings and numbers
            "TJ" => {
                if let Some(arr_obj) = op.operands.first_mut() {
                    if let Ok(arr) = arr_obj.as_array_mut() {
                        for elem in arr.iter_mut() {
                            redact_operand_string(elem);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    match content.encode() {
        // lopdf's content-stream encoder cannot round-trip inline images (`BI`/
        // `ID`/`EI`): it serialises the operand as a generic PDF stream object
        // instead of the special inline-image syntax, which other content-stream
        // parsers (including lopdf itself) then fail to read back. `decode`
        // (non-strict) silently ignores trailing bytes it can't parse, so it
        // won't catch this — use `decode_strict`, which requires the whole
        // buffer to parse, to verify the encoded bytes are actually safe before
        // trusting them. If not, leave this stream's original bytes untouched
        // rather than emit a PDF that downstream tools can't open.
        Ok(encoded) if Content::decode_strict(&encoded).is_ok() => {
            stream.set_plain_content(encoded);
            if !uncompressed {
                let _ = stream.compress();
            }
        }
        _ => {
            eprintln!(
                "warning: content stream {stream_id:?} could not be safely re-encoded \
                 (likely an inline image) — leaving it un-redacted to avoid corrupting the PDF"
            );
            stream.set_plain_content(raw);
        }
    }

    doc.objects.insert(stream_id, Object::Stream(stream));
}

/// Redact a single operand `Object` if it is a `String`.
fn redact_operand_string(obj: &mut Object) {
    if let Object::String(bytes, format) = obj {
        *bytes = redact_string_object(bytes, format);
    }
}

// ---------------------------------------------------------------------------
// Helper: consume Object into Stream (Object has no try_into_stream in older
// lopdf — so we match manually).
// ---------------------------------------------------------------------------

trait IntoStream {
    fn try_into_stream(self) -> Result<Stream, Object>;
}

impl IntoStream for Object {
    fn try_into_stream(self) -> Result<Stream, Object> {
        match self {
            Object::Stream(s) => Ok(s),
            other => Err(other),
        }
    }
}
