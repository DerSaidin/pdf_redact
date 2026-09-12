# PDF Redactor

A command-line tool that redacts alphanumeric content from PDF files for public test data while preserving document structure and styling.

## Language

**Redaction**:
The 1:1 character-count preserving replacement of alphanumeric characters (`A-Z` to `A`, `a-z` to `a`, `0-9` to `0`), leaving punctuation, whitespace, and layout metrics unaltered.
_Avoid_: Masking, sanitization, blacking out

**Text-Bearing Structure**:
Any PDF object or dictionary containing human-readable strings, specifically page content streams, Form XObjects, Document Information (`/Info`), bookmarks (`/Outlines`), annotations (`/Annots`), interactive form fields (`/AcroForm`), and XMP metadata streams (`/Metadata`).
_Avoid_: Text node, string container

**Content Stream**:
A PDF data stream containing operator instructions (such as `Tj`, `TJ`, `'`, `"`, path drawing, and graphics state) defining the visible page layout and graphics on pages and Form XObjects.
_Avoid_: Page code, stream text

**Dual-Layer Redaction**:
Applying character replacement concurrently to raw content stream text operators and font `/ToUnicode` CMaps so both token-level parsers and semantic text extractors observe redacted values.
_Avoid_: Surface redaction, stream-only redaction

**XMP Metadata Redaction**:
Parsing and replacing alphanumeric text within both the XML text nodes and the attribute *values* of `/Metadata` streams (e.g. `xmp:CreatorTool`, `xmp:CreateDate`, `xmpMM:DocumentID`, `xmpMM:History` events), while leaving element tag names, attribute names, and namespace declarations (`xmlns`/`xmlns:*`) untouched so the packet stays well-formed.
_Avoid_: XML stripping, metadata deletion, attribute-name rewriting

**Visual Asset Preservation**:
The explicit non-modification of raster image streams (`/Image`) and vector path operations to preserve graphical styling and image filter structures.
_Avoid_: Image sanitization, visual scrubbing
