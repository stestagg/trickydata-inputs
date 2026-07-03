#!/usr/bin/env python3
"""Build the GitHub Pages root index for versioned report directories."""

from __future__ import annotations

import argparse
import html
import re
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("pages_dir", type=Path)
    parser.add_argument("--latest", required=True)
    args = parser.parse_args()

    pages_dir: Path = args.pages_dir
    pages_dir.mkdir(parents=True, exist_ok=True)
    versions = sorted(
        (
            path.name
            for path in pages_dir.iterdir()
            if path.is_dir() and re.fullmatch(r"v[0-9A-Za-z][0-9A-Za-z._-]*", path.name)
        ),
        key=_version_key,
        reverse=True,
    )
    if args.latest not in versions:
        versions.insert(0, args.latest)

    links = "\n".join(
        f'      <li><a href="{html.escape(version)}/index.html">{html.escape(version)}</a></li>'
        for version in versions
    )
    latest = html.escape(args.latest)
    (pages_dir / "index.html").write_text(
        f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>trickydata report versions</title>
  <style>
    :root {{ color-scheme: light dark; font-family: system-ui, sans-serif; }}
    body {{ margin: 2rem auto; max-width: 42rem; padding: 0 1rem; line-height: 1.5; }}
    a {{ color: CanvasText; }}
  </style>
</head>
<body>
  <main>
    <h1>trickydata report versions</h1>
    <p>Latest: <a href="{latest}/index.html">{latest}</a></p>
    <ul>
{links}
    </ul>
  </main>
</body>
</html>
""",
        encoding="utf-8",
    )
    (pages_dir / ".nojekyll").write_text("", encoding="utf-8")


def _version_key(version: str) -> tuple:
    parts = re.split(r"([0-9]+)", version.removeprefix("v"))
    return tuple((0, int(part)) if part.isdigit() else (1, part) for part in parts)


if __name__ == "__main__":
    main()
