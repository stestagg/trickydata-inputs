"""Load a compiled corpus (``trickydata-index.json`` + ``trickydata.bin``) into
an in-memory model the templates render from.

The index is the source of truth for metadata; the bin holds each input's raw
payload bytes, sliced out by ``offset``/``length``. Optional fields are kept as a
plain dict (``meta``) keyed by the schema field name, so rendering stays
schema-driven rather than hard-coding which fields exist.
"""

from __future__ import annotations

import json
import posixpath
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

# Metadata keys that the model lifts out of the raw dict for first-class handling.
# Everything else in ``meta`` is generic passthrough rendered via the schema.
_NAME = "name"
_PATH = "path"


@dataclass
class PairLink:
    """A resolved link to another input in the corpus."""

    name: str
    hint: str


@dataclass
class Input:
    """One input: its metadata, source path, and decoded payload bytes."""

    name: str
    path: str
    payload: bytes
    meta: dict  # full metadata dict from the index, keyed by schema field name
    pairs: list[PairLink] = field(default_factory=list)

    @property
    def description(self) -> str:
        return self.meta.get("description", "")

    @property
    def format(self) -> str:
        return self.meta.get("format", "")

    @property
    def tags(self) -> list[str]:
        return list(self.meta.get("tags") or [])

    @property
    def size(self) -> int:
        return len(self.payload)


@dataclass
class Corpus:
    """The whole compiled corpus plus derived indexes (tags)."""

    version: str
    inputs: list[Input]
    tag_counts: Counter  # tag -> number of inputs
    tag_index: dict[str, list[str]]  # tag -> sorted input names

    def by_name(self) -> dict[str, Input]:
        return {i.name: i for i in self.inputs}


def load(index_path: str, bin_path: str | None = None) -> Corpus:
    """Read the index + bin pair and build a :class:`Corpus`.

    ``bin_path`` defaults to the index's own ``bin`` field, resolved next to the
    index file (matching how ``make-index`` writes the pair side by side).
    """
    index_file = Path(index_path)
    with index_file.open(encoding="utf-8") as fh:
        doc = json.load(fh)

    if bin_path is None:
        bin_path = str(index_file.parent / doc.get("bin", "trickydata.bin"))
    blob = Path(bin_path).read_bytes()

    raw_entries = doc.get("inputs", [])
    inputs: list[Input] = []
    for entry in raw_entries:
        offset = entry["offset"]
        length = entry["length"]
        inputs.append(
            Input(
                name=entry[_NAME],
                path=entry.get(_PATH, ""),
                payload=blob[offset : offset + length],
                meta=entry,
            )
        )

    _resolve_pairs(inputs)
    tag_counts, tag_index = _index_tags(inputs)

    return Corpus(
        version=doc.get("version", "dev"),
        inputs=inputs,
        tag_counts=tag_counts,
        tag_index=tag_index,
    )


def _resolve_pairs(inputs: list[Input]) -> None:
    """Resolve each ``pair[].with`` (a path to another ``.input``, relative to the
    linking entry's own ``path``) to the target input's name, for cross-linking.

    Paths in the index may be prefixed differently depending on where
    ``make-index`` ran (``inputs/...`` vs ``../inputs/...``), so we match on the
    lexically-normalized path rather than anything absolute.
    """
    name_by_path: dict[str, str] = {}
    for inp in inputs:
        if inp.path:
            name_by_path[_norm(inp.path)] = inp.name

    for inp in inputs:
        base_dir = posixpath.dirname(inp.path)
        for link in inp.meta.get("pair") or []:
            target_path = _norm(posixpath.join(base_dir, link["with"]))
            target_name = name_by_path.get(target_path)
            if target_name is not None:
                inp.pairs.append(PairLink(name=target_name, hint=link.get("hint", "")))


def _index_tags(inputs: list[Input]) -> tuple[Counter, dict[str, list[str]]]:
    """Build tag -> count and tag -> [input names] maps from the data."""
    counts: Counter = Counter()
    index: dict[str, list[str]] = {}
    for inp in inputs:
        for tag in inp.tags:
            counts[tag] += 1
            index.setdefault(tag, []).append(inp.name)
    for names in index.values():
        names.sort()
    return counts, index


def _norm(path: str) -> str:
    """Lexically normalize a forward-slash path (resolve ``.``/``..``)."""
    return posixpath.normpath(path.replace("\\", "/"))
