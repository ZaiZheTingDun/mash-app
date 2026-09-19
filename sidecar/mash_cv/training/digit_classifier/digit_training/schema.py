"""Versioned JSONL schema for digit-classifier samples.

Every crop keeps the identity of its source screenshot. Dataset splits operate
on that parent identity so near-identical crops from one frame cannot leak
between training and evaluation.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass, replace
import hashlib
import json
from pathlib import Path, PurePosixPath
import tempfile
from typing import Any, Iterable, Mapping, Sequence


SCHEMA_VERSION = 1
DIGIT_LABELS = tuple(str(digit) for digit in range(10))
INVALID_LABEL = "invalid"
VALID_LABELS = frozenset((*DIGIT_LABELS, INVALID_LABEL))
VALID_SPLITS = frozenset(("train", "validation", "test"))
VALID_SERVERS = frozenset(("cn", "jp", "shared"))


def _required_string(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field} must be a non-empty string")
    return value.strip()


def _relative_image_path(value: Any) -> str:
    raw = _required_string(value, "image")
    path = PurePosixPath(raw.replace("\\", "/"))
    if path.is_absolute() or ".." in path.parts:
        raise ValueError("image must be a portable path relative to the dataset root")
    return str(path)


def _optional_resolution(value: Any) -> tuple[int, int] | None:
    if value is None:
        return None
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise ValueError("resolution must be [width, height]")
    if len(value) != 2:
        raise ValueError("resolution must contain exactly two integers")
    width, height = value
    if not isinstance(width, int) or not isinstance(height, int):
        raise ValueError("resolution must contain exactly two integers")
    if width <= 0 or height <= 0:
        raise ValueError("resolution values must be positive")
    return width, height


def _optional_bbox(value: Any) -> tuple[int, int, int, int] | None:
    if value is None:
        return None
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise ValueError("sourceBboxPx must be [x, y, width, height]")
    if len(value) != 4 or any(not isinstance(item, int) for item in value):
        raise ValueError("sourceBboxPx must contain exactly four integers")
    x, y, width, height = value
    if x < 0 or y < 0 or width <= 0 or height <= 0:
        raise ValueError("sourceBboxPx must have non-negative origin and positive size")
    return x, y, width, height


@dataclass(frozen=True)
class DigitSample:
    """One cropped digit candidate and the metadata needed for safe splits."""

    sample_id: str
    image: str
    source: str
    style: str
    server: str
    parent_id: str
    label: str | None = None
    resolution: tuple[int, int] | None = None
    source_bbox_px: tuple[int, int, int, int] | None = None
    split: str | None = None
    notes: str | None = None

    @classmethod
    def from_dict(cls, raw: Mapping[str, Any]) -> "DigitSample":
        version = raw.get("schemaVersion", SCHEMA_VERSION)
        if version != SCHEMA_VERSION:
            raise ValueError(
                f"unsupported schemaVersion {version!r}; expected {SCHEMA_VERSION}"
            )

        label = raw.get("label")
        if label is not None and label not in VALID_LABELS:
            raise ValueError(f"label must be one of {sorted(VALID_LABELS)} or null")

        split = raw.get("split")
        if split is not None and split not in VALID_SPLITS:
            raise ValueError(f"split must be one of {sorted(VALID_SPLITS)} or null")

        server = _required_string(raw.get("server"), "server")
        if server not in VALID_SERVERS:
            raise ValueError(f"server must be one of {sorted(VALID_SERVERS)}")

        notes = raw.get("notes")
        if notes is not None and not isinstance(notes, str):
            raise ValueError("notes must be a string or null")

        return cls(
            sample_id=_required_string(raw.get("id"), "id"),
            image=_relative_image_path(raw.get("image")),
            source=_required_string(raw.get("source"), "source"),
            style=_required_string(raw.get("style"), "style"),
            server=server,
            parent_id=_required_string(raw.get("parentId"), "parentId"),
            label=label,
            resolution=_optional_resolution(raw.get("resolution")),
            source_bbox_px=_optional_bbox(raw.get("sourceBboxPx")),
            split=split,
            notes=notes,
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "schemaVersion": SCHEMA_VERSION,
            "id": self.sample_id,
            "image": self.image,
            "label": self.label,
            "source": self.source,
            "style": self.style,
            "server": self.server,
            "parentId": self.parent_id,
            "resolution": list(self.resolution) if self.resolution else None,
            "sourceBboxPx": list(self.source_bbox_px)
            if self.source_bbox_px
            else None,
            "split": self.split,
            "notes": self.notes,
        }


def load_manifest(path: Path) -> list[DigitSample]:
    samples: list[DigitSample] = []
    seen_ids: set[str] = set()
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                raw = json.loads(line)
                sample = DigitSample.from_dict(raw)
            except (json.JSONDecodeError, TypeError, ValueError) as error:
                raise ValueError(f"{path}:{line_number}: {error}") from error
            if sample.sample_id in seen_ids:
                raise ValueError(
                    f"{path}:{line_number}: duplicate sample id {sample.sample_id!r}"
                )
            seen_ids.add(sample.sample_id)
            samples.append(sample)
    return samples


def save_manifest(path: Path, samples: Iterable[DigitSample]) -> None:
    """Atomically replace a manifest after validating all sample IDs."""

    materialized = list(samples)
    ids = [sample.sample_id for sample in materialized]
    if len(ids) != len(set(ids)):
        raise ValueError("manifest contains duplicate sample IDs")

    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w",
        encoding="utf-8",
        dir=path.parent,
        prefix=f".{path.name}.",
        suffix=".tmp",
        delete=False,
    ) as handle:
        temporary_path = Path(handle.name)
        for sample in materialized:
            handle.write(
                json.dumps(sample.to_dict(), ensure_ascii=False, sort_keys=True)
                + "\n"
            )
    temporary_path.replace(path)


def assign_grouped_splits(
    samples: Sequence[DigitSample],
    *,
    seed: int,
    train_ratio: float = 0.8,
    validation_ratio: float = 0.1,
    test_ratio: float = 0.1,
) -> list[DigitSample]:
    """Assign deterministic splits while keeping every parent in one split."""

    ratios = (train_ratio, validation_ratio, test_ratio)
    if any(ratio < 0.0 for ratio in ratios):
        raise ValueError("split ratios cannot be negative")
    if abs(sum(ratios) - 1.0) > 1e-9:
        raise ValueError("split ratios must sum to 1.0")

    parent_ids = sorted(
        {sample.parent_id for sample in samples if sample.label is not None},
        key=lambda parent_id: hashlib.sha256(
            f"{seed}:{parent_id}".encode("utf-8")
        ).digest(),
    )
    parent_count = len(parent_ids)
    train_end = round(parent_count * train_ratio)
    validation_end = train_end + round(parent_count * validation_ratio)
    split_by_parent = {
        parent_id: (
            "train"
            if index < train_end
            else "validation"
            if index < validation_end
            else "test"
        )
        for index, parent_id in enumerate(parent_ids)
    }

    return [
        replace(
            sample,
            split=split_by_parent.get(sample.parent_id)
            if sample.label is not None
            else None,
        )
        for sample in samples
    ]


def manifest_summary(samples: Sequence[DigitSample]) -> dict[str, Any]:
    labels = Counter(sample.label or "unlabeled" for sample in samples)
    splits = Counter(sample.split or "unassigned" for sample in samples)
    return {
        "samples": len(samples),
        "labeled": sum(sample.label is not None for sample in samples),
        "unlabeled": sum(sample.label is None for sample in samples),
        "labels": dict(sorted(labels.items())),
        "sources": dict(sorted(Counter(sample.source for sample in samples).items())),
        "styles": dict(sorted(Counter(sample.style for sample in samples).items())),
        "servers": dict(sorted(Counter(sample.server for sample in samples).items())),
        "splits": dict(sorted(splits.items())),
        "parents": len({sample.parent_id for sample in samples}),
    }
