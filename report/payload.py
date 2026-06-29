"""Turn raw payload bytes into the representations shown on an input page.

Three views: UTF-8 (lossy, so invalid bytes are visible), a hex-editor style
dump, and a downloadable copy of the exact bytes. ``default_tab`` picks which one
opens first based on the input's ``format``.
"""

from __future__ import annotations

import base64
from dataclasses import dataclass

# How many bytes per hexdump row, and where the visual gap between groups falls.
_ROW = 16
_GROUP = 8

# Which payload tab opens by default for each `format`. Derived from the format
# enum; unknown formats fall back to the hex view (always safe for any bytes).
_DEFAULT_TAB = {
    "utf8": "utf8",
    "utf8-strip": "utf8",
    "hex": "hex",
    "file": "file",
}
_FALLBACK_TAB = "hex"


@dataclass(frozen=True)
class HexRow:
    """One row of a hexdump: byte offset, the hex cells, and the ASCII gutter."""

    offset: str  # e.g. "00000010"
    cells: list[str]  # 16 entries; "" pads a short final row
    ascii: str  # printable chars, "." for the rest


def default_tab(format_: str) -> str:
    """The tab id to open first for an input of the given ``format``."""
    return _DEFAULT_TAB.get(format_, _FALLBACK_TAB)


def as_utf8_replace(data: bytes) -> str:
    """Decode as UTF-8, replacing invalid sequences with U+FFFD so malformed
    bytes are visible rather than raising."""
    return data.decode("utf-8", errors="replace")


def hexdump(data: bytes) -> list[HexRow]:
    """Build hex-editor style rows: 16 bytes each, split into two 8-byte groups,
    with a printable-ASCII gutter."""
    rows: list[HexRow] = []
    for start in range(0, len(data), _ROW):
        chunk = data[start : start + _ROW]
        cells = [f"{b:02x}" for b in chunk]
        cells += [""] * (_ROW - len(cells))  # pad the final row for alignment
        gutter = "".join(chr(b) if 0x20 <= b < 0x7F else "." for b in chunk)
        rows.append(HexRow(offset=f"{start:08x}", cells=cells, ascii=gutter))
    return rows


def hex_group_size() -> int:
    """Bytes per visual group in the hexdump (for template column logic)."""
    return _GROUP


def as_data_url(data: bytes, mime: str = "application/octet-stream") -> str:
    """A base64 ``data:`` URL embedding the exact bytes, for the download link."""
    b64 = base64.b64encode(data).decode("ascii")
    return f"data:{mime};base64,{b64}"
