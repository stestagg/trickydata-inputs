#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["click", "jinja2", "pyyaml"]
# ///
"""``trickydata-report`` — render a compiled input corpus into a static site.

Consumes a compiled ``trickydata-index.json`` + ``trickydata.bin`` pair (produced
by the Rust ``compile`` tool) plus ``frontmatter-schema.yaml``, and writes a
self-contained HTML+JS report. Run with uv (deps are declared inline above):

    uv run report/main.py --out report-site/
"""

from __future__ import annotations

from pathlib import Path

import click

import render
from model import load
from schema import load_schema

_HERE = Path(__file__).resolve().parent
_REPO = _HERE.parent


@click.command()
@click.option(
    "--index",
    "index_path",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    default=_REPO / "trickydata-index.json",
    show_default=True,
    help="Compiled input index (trickydata-index.json).",
)
@click.option(
    "--bin",
    "bin_path",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    default=None,
    help="Payload blob; defaults to the index's own 'bin' field, next to the index.",
)
@click.option(
    "--schema",
    "schema_path",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    default=_REPO / "frontmatter-schema.yaml",
    show_default=True,
    help="Frontmatter JSON Schema, used to label/order metadata.",
)
@click.option(
    "--out",
    "out_dir",
    type=click.Path(file_okay=False, path_type=Path),
    default=Path("report-site"),
    show_default=True,
    help="Output directory for the generated site.",
)
def main(index_path: Path, bin_path: Path | None, schema_path: Path, out_dir: Path) -> None:
    """Render the corpus at INDEX into a static site under OUT."""
    corpus = load(str(index_path), str(bin_path) if bin_path else None)
    schema = load_schema(str(schema_path))

    count = render.render_site(
        corpus,
        schema,
        out_dir,
        template_dir=_HERE / "templates",
        static_dir=_HERE / "static",
    )

    click.echo(
        f"Rendered {count} input(s) (version {corpus.version}, "
        f"{len(corpus.tag_counts)} tags) -> {out_dir / 'index.html'}"
    )


if __name__ == "__main__":
    main()
