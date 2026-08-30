#!/usr/bin/env python3
"""
Generate docs/ERRORS.md from contract error enums.

Parses:
  - contracts/raffle-instance/src/lib.rs — `Error`
  - contracts/raffle-factory/src/lib.rs — `ContractError`

Fails when duplicate discriminants, undeclared-but-used variants, or missing
doc comments are detected.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class ErrorVariant:
    code: int
    name: str
    doc: str


@dataclass(frozen=True)
class ErrorEnumSpec:
    contract: str
    enum_name: str
    source_path: str
    variants: list[ErrorVariant]


ENUM_PATTERN = re.compile(
    r"#\[contracterror\]\s*"
    r"#\[derive\([^\]]*\)\]\s*"
    r"pub enum (\w+) \{(.*?)\n\}",
    re.DOTALL,
)

VARIANT_PATTERN = re.compile(
    r"(?P<name>\w+)\s*=\s*(?P<code>\d+)\s*,",
    re.MULTILINE,
)


def extract_doc_comment(block: str, variant_name: str, variant_start: int) -> str:
    """Return the /// doc comment immediately preceding a variant."""
    prefix = block[:variant_start]
    lines: list[str] = []
    for line in reversed(prefix.rstrip().splitlines()):
        stripped = line.strip()
        if stripped.startswith("///"):
            lines.append(stripped[3:].strip())
        elif stripped == "":
            continue
        else:
            break
    if not lines:
        return ""
    return " ".join(reversed(lines)).strip()


def parse_enum(content: str, enum_name: str) -> list[ErrorVariant]:
    match = ENUM_PATTERN.search(content)
    if not match or match.group(1) != enum_name:
        raise ValueError(f"Could not find #[contracterror] pub enum {enum_name}")

    enum_body = match.group(2)
    variants: list[ErrorVariant] = []
    seen_codes: dict[int, str] = {}

    for variant_match in VARIANT_PATTERN.finditer(enum_body):
        name = variant_match.group("name")
        code = int(variant_match.group("code"))
        if code in seen_codes:
            raise ValueError(
                f"Duplicate discriminant {code} in {enum_name}: "
                f"{seen_codes[code]} and {name}"
            )
        seen_codes[code] = name
        doc = extract_doc_comment(enum_body, name, variant_match.start("name"))
        if not doc:
            raise ValueError(f"Missing doc comment for {enum_name}::{name}")
        variants.append(ErrorVariant(code=code, name=name, doc=doc))

    variants.sort(key=lambda v: v.code)
    return variants


def find_used_variants(content: str, enum_name: str, declared: set[str]) -> list[str]:
    """Find Error::Variant references that are not declared in the enum."""
    pattern = re.compile(rf"{enum_name}::(\w+)")
    used = set(pattern.findall(content))
    return sorted(name for name in used if name not in declared)


def load_spec(path: Path, contract: str, enum_name: str) -> ErrorEnumSpec:
    content = path.read_text()
    variants = parse_enum(content, enum_name)
    declared = {v.name for v in variants}
    missing = find_used_variants(content, enum_name, declared)
    if missing:
        raise ValueError(
            f"{enum_name} in {path}: referenced but not declared: {', '.join(missing)}"
        )
    rel = path.relative_to(path.parents[2])
    return ErrorEnumSpec(
        contract=contract,
        enum_name=enum_name,
        source_path=str(rel),
        variants=variants,
    )


def frontend_message(doc: str) -> str:
    first = doc.split(".")[0].strip()
    if not first:
        return '"Unknown error"'
    if not first.endswith('"'):
        return f'"{first}"'
    return first


def render_table(spec: ErrorEnumSpec) -> str:
    lines = [
        f"Source enum: `{spec.enum_name}` in [`{spec.source_path}`]({spec.source_path})",
        "",
        "| Code | Error | Description | Contract | Frontend Message |",
        "| ---- | ----- | ----------- | -------- | ---------------- |",
    ]
    for variant in spec.variants:
        msg = frontend_message(variant.doc)
        lines.append(
            f"| {variant.code} | `{variant.name}` | {variant.doc} | "
            f"{spec.contract} | {msg} |"
        )
    return "\n".join(lines)


def render_doc(instance: ErrorEnumSpec, factory: ErrorEnumSpec) -> str:
    return f"""# Error Codes Documentation

This document is generated from the contract error enums. Regenerate with:

```bash
python3 scripts/generate_error_docs.py
```

Sources:
- `{instance.enum_name}` — `{instance.source_path}`
- `{factory.enum_name}` — `{factory.source_path}`

## Table of Contents

- [Instance Contract Errors](#instance-contract-errors)
- [Factory Contract Errors](#factory-contract-errors)

---

## Instance Contract Errors

The instance contract (`RaffleInstance`) handles individual raffle operations.

{render_table(instance)}

---

## Factory Contract Errors

The factory contract (`RaffleFactory`) manages raffle creation.

{render_table(factory)}
"""


def main() -> None:
    repo_root = Path(__file__).parent.parent
    instance_path = repo_root / "contracts" / "raffle-instance" / "src" / "lib.rs"
    factory_path = repo_root / "contracts" / "raffle-factory" / "src" / "lib.rs"
    errors_doc = repo_root / "docs" / "ERRORS.md"

    try:
        instance = load_spec(instance_path, "RaffleInstance", "Error")
        factory = load_spec(factory_path, "RaffleFactory", "ContractError")
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        sys.exit(1)

    errors_doc.write_text(render_doc(instance, factory))
    print(f"Wrote {errors_doc}")


if __name__ == "__main__":
    main()
