# Character-Count Preserving Redaction

To provide reliable public test fixtures for PDF parsing libraries that test text extraction, bounding boxes, and layout metrics, alphanumeric characters are replaced 1:1 (`a-z` to `a`, `A-Z` to `A`, `0-9` to `0`) rather than with arbitrary fixed strings. This preserves string lengths, word boundaries, font kerning offsets in `TJ` operators, and document layout without altering structural object streams.
