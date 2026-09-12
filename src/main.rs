use std::path::PathBuf;

use clap::Parser;
use lopdf::Document;
use pdf_redact::redact_document;

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

    redact_document(&mut doc, args.uncompressed);

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
