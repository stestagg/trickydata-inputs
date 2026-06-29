"""Render the corpus to a static site with Jinja2.

Pages: ``index.html`` (home table), ``tags.html`` (tag explorer), and one
``inputs/<name>.html`` per input. ``static/`` is copied verbatim. Pages use a
``root`` relative prefix so the site works opened directly from disk (no server),
regardless of nesting.
"""

from __future__ import annotations

import shutil
from pathlib import Path

from jinja2 import Environment, FileSystemLoader, select_autoescape

import payload
from model import Corpus, Input
from schema import SchemaInfo

# Fields handled specially on the input page; everything else in schema order is
# rendered as a generic labelled key/value row.
_SPECIAL_FIELDS = {"name", "description", "tags", "pair", "unicode-meta"}


def render_site(
    corpus: Corpus,
    schema: SchemaInfo,
    out_dir: Path,
    *,
    template_dir: Path,
    static_dir: Path,
) -> int:
    """Render every page into ``out_dir`` and copy static assets. Returns the
    number of input pages written."""
    env = _build_env(template_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    # Home + tag explorer (top level: root prefix is empty).
    _write(
        out_dir / "index.html",
        env.get_template("index.html").render(
            root="", corpus=corpus, schema=schema, page="home"
        ),
    )
    _write(
        out_dir / "tags.html",
        env.get_template("tags.html").render(
            root="",
            corpus=corpus,
            tags=sorted(corpus.tag_counts.items(), key=lambda kv: (-kv[1], kv[0])),
            page="tags",
        ),
    )

    # One page per input (nested one level: root prefix is "../").
    inputs_dir = out_dir / "inputs"
    inputs_dir.mkdir(parents=True, exist_ok=True)
    input_tmpl = env.get_template("input.html")
    for inp in corpus.inputs:
        _write(
            inputs_dir / f"{inp.name}.html",
            input_tmpl.render(
                root="../",
                corpus=corpus,
                schema=schema,
                inp=inp,
                page="input",
                meta_rows=_meta_rows(inp, schema),
                view=_payload_view(inp),
            ),
        )

    # Static assets.
    dest_static = out_dir / "static"
    if dest_static.exists():
        shutil.rmtree(dest_static)
    shutil.copytree(static_dir, dest_static)

    return len(corpus.inputs)


def _build_env(template_dir: Path) -> Environment:
    env = Environment(
        loader=FileSystemLoader(str(template_dir)),
        autoescape=select_autoescape(["html"]),
        trim_blocks=True,
        lstrip_blocks=True,
    )
    env.filters["humansize"] = _humansize
    return env


def _meta_rows(inp: Input, schema: SchemaInfo) -> list[dict]:
    """Generic scalar metadata rows (format, decode-as, mime-type, licence, ...)
    in schema order, labelled/described from the schema. Special fields and any
    field absent from this input are skipped."""
    rows: list[dict] = []
    for name in schema.field_order:
        if name in _SPECIAL_FIELDS or name not in inp.meta:
            continue
        field = schema.get(name)
        rows.append(
            {
                "name": name,
                "title": schema.title(name),
                "description": field.description if field else None,
                "value": inp.meta[name],
            }
        )
    return rows


def _payload_view(inp: Input) -> dict:
    """Precompute the three payload representations for an input page."""
    mime = inp.meta.get("mime-type") or "application/octet-stream"
    return {
        "default_tab": payload.default_tab(inp.format),
        "utf8": payload.as_utf8_replace(inp.payload),
        "hex_rows": payload.hexdump(inp.payload),
        "group": payload.hex_group_size(),
        "data_url": payload.as_data_url(inp.payload, mime),
    }


def _humansize(n: int) -> str:
    if n < 1024:
        return f"{n} B"
    if n < 1024 * 1024:
        return f"{n / 1024:.1f} KiB"
    return f"{n / (1024 * 1024):.1f} MiB"


def _write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")
