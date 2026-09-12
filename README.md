# pdf-redact

A command-line tool that redacts all alphanumeric content from a PDF file while leaving its structure, layout, and styling exactly intact.

**Purpose:** produce redacted copies of real-world PDFs that can be safely shared as public test fixtures for PDF parsing libraries.

> [!CAUTION]
> 1) This is vibe coded.
> 2) PDF is a complex format, and I only tested on a couple of example documents. It is very possible some place carrying content is not redacted.
> 3) Does not (currently) redact images.
>
> Do not trust this tool alone. You should verify that no information you care about leaked into the output.

## How it works

Every alphanumeric character is replaced 1-to-1 with a neutral placeholder:

| Input | Output |
|-------|--------|
| `a–z` | `a`    |
| `A–Z` | `A`    |
| `0–9` | `0`    |
| Non-ASCII letter | `a` / `A` (case-preserving) |
| Non-ASCII digit | `0` |
| Whitespace, punctuation, symbols | **unchanged** |

Because replacement is 1-to-1, string lengths never change. Font kerning arrays (`TJ` offsets), text coordinates, word boundaries, and all layout metrics are preserved exactly as they were in the original.

## What gets redacted

`pdf-redact` walks every text-bearing structure in the PDF object graph:

| Structure | What is redacted |
|-----------|-----------------|
| Page content streams | String operands of `Tj`, `TJ`, `'`, `"` operators |
| Form XObjects | Same text operators inside reusable content streams |
| Document metadata (`/Info`) | `/Title`, `/Author`, `/Subject`, `/Keywords`, `/Creator`, etc. |
| Bookmarks (`/Outlines`) | Bookmark title strings |
| Annotations & form fields (`/Annots`, `/AcroForm`) | Field values (`/V`), default values (`/DV`), annotation contents |
| `/ToUnicode` CMap streams | Destination Unicode code points, so text extractors also see redacted characters |
| XMP metadata (`/Metadata`) | Text nodes and attribute values inside the XMP XML (e.g. `xmp:CreatorTool`, `xmp:CreateDate`, `xmpMM:DocumentID`, history events), preserving element names, attribute names, and namespace declarations |

The following are **never modified**: raster images, vector paths, font programs, colour spaces, coordinate streams, or any PDF structural byte.

## Installation

```sh
cargo install --path .
```

Or build a release binary directly:

```sh
cargo build --release
# binary at target/release/pdf-redact
```

**Minimum supported Rust version:** 1.88 (required by lopdf 0.45)

## Usage

```sh
# Produces input.redacted.pdf in the same directory
pdf-redact input.pdf

# Custom output path
pdf-redact input.pdf --output /path/to/output.pdf
pdf-redact input.pdf -o /path/to/output.pdf

# Write uncompressed content streams (useful for inspecting fixture bytes)
pdf-redact input.pdf --uncompressed

# Combine flags
pdf-redact input.pdf --uncompressed -o debug.pdf
```

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success — output path printed to stdout |
| `1` | Failure — descriptive message printed to stderr |

## Encrypted PDFs

If the PDF has a standard security handler with a blank user password (the most common case), `pdf-redact` decrypts it automatically. If the file requires a non-empty password, the tool exits with code `1` and an error message.

## Design decisions

See [`docs/adr/`](docs/adr/) for the Architecture Decision Records that capture key choices:

- [ADR 0001](docs/adr/0001-character-count-preserving-redaction.md) — Why 1-to-1 character replacement instead of fixed-length placeholders
- [ADR 0002](docs/adr/0002-dual-layer-content-and-tounicode-redaction.md) — Why both content stream bytes and `/ToUnicode` CMaps are redacted

Domain terminology is defined in [`CONTEXT.md`](CONTEXT.md).

## Dependencies

| Crate | Version | Role |
|-------|---------|------|
| [lopdf](https://github.com/J-F-Liu/lopdf) | 0.45 | PDF parsing, object graph traversal, and writing |
| [quick-xml](https://github.com/tafia/quick-xml) | 0.42 | XMP metadata stream redaction |
| [clap](https://github.com/clap-rs/clap) | 4 | CLI argument parsing |

## Limitations

- **Embedded raster images are not touched.** If the source PDF contains images of text (e.g. scanned pages), those pixels are preserved as-is.
- **Encrypted PDFs with non-empty passwords** are not supported; the tool exits with an error.
- **Incremental updates** in PDFs are fully decoded and re-written as a linearised document — the on-disk incremental structure is not preserved.
