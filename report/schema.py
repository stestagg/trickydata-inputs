"""Read ``frontmatter-schema.yaml`` so rendering is driven by the schema rather
than a hard-coded field list.

The schema is JSON Schema (draft 2020-12). We only need its ``properties`` map:
each field's human title/description, any ``enum`` of allowed values, and which
fields are ``required``. Everything the report shows about an input's metadata is
labelled and ordered from here, so adding a field to the schema (and the corpus)
surfaces it in the report without code changes.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import yaml


@dataclass(frozen=True)
class Field:
    """One metadata property as described by the schema."""

    name: str
    description: str | None
    enum: list[str] | None
    required: bool

    @property
    def title(self) -> str:
        """A human label for the field. The schema gives descriptions but no
        explicit titles, so derive one from the (kebab-case) key."""
        return self.name.replace("-", " ").replace("_", " ").title()


@dataclass(frozen=True)
class SchemaInfo:
    """The subset of the frontmatter schema the report renders from."""

    fields: dict[str, Field] = field(default_factory=dict)

    @property
    def field_order(self) -> list[str]:
        """Schema declaration order — used to lay out metadata consistently."""
        return list(self.fields.keys())

    def get(self, name: str) -> Field | None:
        return self.fields.get(name)

    def title(self, name: str) -> str:
        f = self.fields.get(name)
        return f.title if f else name.replace("-", " ").replace("_", " ").title()

    def enum(self, name: str) -> list[str] | None:
        f = self.fields.get(name)
        return f.enum if f else None


def load_schema(path: str) -> SchemaInfo:
    """Parse the YAML schema into a :class:`SchemaInfo`."""
    with open(path, encoding="utf-8") as fh:
        doc = yaml.safe_load(fh)

    required = set(doc.get("required", []))
    fields: dict[str, Field] = {}
    for name, spec in (doc.get("properties") or {}).items():
        spec = spec or {}
        fields[name] = Field(
            name=name,
            description=_clean(spec.get("description")),
            enum=list(spec["enum"]) if isinstance(spec.get("enum"), list) else None,
            required=name in required,
        )
    return SchemaInfo(fields=fields)


def _clean(text: str | None) -> str | None:
    """JSON Schema descriptions here use YAML folded scalars, which collapse to a
    single line with a trailing newline; tidy that for display."""
    if text is None:
        return None
    return " ".join(text.split()).strip() or None
