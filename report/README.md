# report

Renders a compiled trickydata corpus into a self-contained static HTML+JS site:
a page per input (metadata + UTF-8 / hex / download payload views), a sortable,
filterable home table, and a tag explorer.

## Usage

Dependencies are declared inline in `main.py` (PEP 723) and resolved by
[uv](https://docs.astral.sh/uv/), so there is nothing to install. The script is
directly executable via its `uv run --script` shebang:

```sh
./report/main.py --out report-site/
```

or equivalently `uv run report/main.py --out report-site/`.

Then open `report-site/index.html` in a browser (no server needed).

### Options

| Option     | Default                            | Purpose                                      |
|------------|------------------------------------|----------------------------------------------|
| `--index`  | `trickydata-index.json`            | Compiled input index.                        |
| `--bin`    | the index's own `bin` field        | Payload blob (resolved next to the index).   |
| `--schema` | `frontmatter-schema.yaml`          | Drives metadata labels/order.                |
| `--out`    | `report-site/`                     | Output directory.                            |

The index + bin pair is produced by the Rust `make-index` tool
(`cargo run --bin make-index`).

## Layout

- `main.py` — CLI entry (click); wires loading to rendering.
- `model.py` — loads index + bin; resolves pair links; indexes tags.
- `schema.py` — reads `frontmatter-schema.yaml` for field labels/enums.
- `payload.py` — UTF-8 / hexdump / base64 payload representations.
- `render.py` — Jinja2 environment and page rendering.
- `templates/`, `static/` — HTML templates and CSS/JS assets.
