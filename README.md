# trickydata-inputs
Input files for the trickydata library

If you're writing code that handles external inputs, this library is designed to provide you with inputs you may not have considered, or that excercise corner cases.

# Structure

Inputs are defined in files with `.input` extension, stored in subdirectories of the `inputs` directory.

Subdirectories should be chosen/named to keep the inputs organised and easy to navigate.  They are not used for the indexing or querying of inputs (that's the job of the input tags).

Each input file has a markdown-style frontmatter section, documented under [Frontmatter](#frontmatter).

The file is processed such that everything before the first '\n---\n' is considered a yaml frontmatter document, and anything after the '\n---\n' is the input (or an error if the input is in a separate file)

Depending on the format value, the actual input data is either then included verbatim in the input file, as bytes after the first '\n---\n', or in a separate file (for example, for binary inputs, in which case, any non-whitespace after the frontmatter is treated as an error).

# Inputs

Each input in this corpus must be justified as being an input that may trigger corner-cases, or is otherwise interesting to test against.  The Description field **must** justify why the input has been included, including hyperlinks where relevant to external references.

Each input must be a single example of a tricky input.  If several inputs could be related, then link them with the `pair` field in the frontmatter.  To support this, it's ok for an input to not actually be a tricky input on its own, if it's used as a baseline or adjunct to another input, where together they form a tricky input pair.

## Safety

No input should attempt to cause active harm to a system running them, for example, inputs that highlight injection bugs must NEVER include harmful payloads, (The classic example being `DROP TABLES`).  Instead, aim to cause an error, exception or fault that is hard to miss, but benign.

## Encoding

Consider that these inputs are designed to trigger corner cases and issues with data handing, therefore, pick the input `format` carefully to ensure that the actual data is encoded correctly, and can be reasonably decoded by the clients.  I.e. it may be better to use hex encoding for some values rather than just the raw bytes, not only to avoid file read issues, but also to make debugging easier, as the source of any decoding issue can be more easily identified if the raw encoding format is more explicit.

## Frontmatter

Each input file begins with a markdown-style frontmatter section that matches the `frontmatter-schema.yaml` schema:

A cut-down example of a frontmatter section is:

```yaml
name: <name of the input> (lowercase, underscore separated descriptive but concise name)
description: <why is this input included in the corpus, what is it testing for.  can be multiline>
tags: <array of tag ids as queried by the indexer>
format: <the format of the input, utf8, utf8-strip, hex, file, etc.>
---
```

### pair

If several inputs could be related, then use the `pair` field to link them by relative path and describe the relationship with an `equal`, `not-equal`, or `tricky` hint.

The `pair` field is an array of links, each with a `with` path (relative to this input file) and a `hint`:

```yaml
pair:
  - with: canonical-cafe-nfd.input
    hint: equal
```

The `hint` is one of:

- `equal` — the two inputs look different but should compare equal.
- `not-equal` — the two inputs look the same but should compare unequal.
- `tricky` — together the two inputs form a tricky case.

Equality hints describe logical equality after decoding, not byte-for-byte
identity. For example, floating-point positive and negative zero are `equal`,
while values that round to the same displayed text but have different exact
numeric values are `not-equal`.

Links are made symmetric when the corpus is compiled: declaring `A → B` automatically adds the reverse `B → A` link (carrying the same hint), so you only need to declare a pairing once.

### decode-as

This indicates the logical type that the input represents, allowing clients to decode the input accurately and as appropriate.  The decode-as values roughly follow rust type naming scheme, but also include specific string and byte encodings.  For more complex structured inputs, either omit this or use `bytes`, and provide a mime-type value.

All numeric types **must** be encoded/decoded as little-endian, and should not include any padding or alignment bytes.

### unicode-meta

When `decode-as` is a Unicode encoding (`utf8`, `utf16`, or `utf32`), the frontmatter **must** include a `unicode-meta` block giving known-good counts of the text's segmentation units, so clients can validate their own segmentation against them. Every count is required:

- `code-units` — number of encoding code units (bytes for `utf8`, 16-bit units for `utf16`, 32-bit units for `utf32`).
- `code-points` — number of Unicode code points.
- `scalar-values` — number of Unicode scalar values (code points excluding surrogates).
- `legacy-grapheme-clusters` — number of legacy grapheme clusters (UAX #29).
- `extended-grapheme-clusters` — number of extended grapheme clusters (UAX #29).

# Encodings

## utf8

The entire input is a utf8 encoded string, take care to ensure that your editor does not convert the encoding or otherwise mangle the input.  The input MUST be valid utf-8, so inputs that test invalid utf-8 should probably use the `hex` format.

## utf8-strip

The input is a utf8 encoded string, but any leading or trailing whitespace characters are stripped.  Whitespace is any character having the `White_Space=True` UCD derived property.  The input MUST be valid utf-8, so inputs that test invalid utf-8 should probably use the `hex` format.

## hex

The input nominally matches:

00 11 22 33 44 55 66 77 88 99 aa bb...

Each pair of characters represent a single byte as a hexadecimal value.  letters are case-insensitive, and whitespace is optional, but recommended.

Whitespace is any character having the `White_Space=True` UCD derived property (spaces, tabs, newlines, etc.).

a # introduces a comment, which continues to the end of the line.  Comments are ignored.

a double quote: '"' character introduces a quoted string which can contain any ascii character (except '"') and is terminated by the next '"' character.  The quoted ascii string is included as bytes in the input.
### Example

```
"GIF89a" # Magic value
03 00    # Width
05 00    # Height
F7       # Global color table marker.
...
```

## file

The contents of the input are stored in a separate file.  By default this file is the basename of the input file (e.g. `inputs/foo/bar.input` file would be: `inputs/foo/bar`).  This can be overridden by specifying a `file-source` field in the frontmatter, which is a relative path to the input file from the location of the input file.

The bytes of the file are read as-is, and are not decoded or otherwise processed.  The file is expected to be a binary file, and may contain any bytes.

Only use `file` where no other format is suitable as, for example, a comment-annotated hex format is easier to understand and debug.
