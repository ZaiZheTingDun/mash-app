"""Build whole-number sequence samples from fixed-slot digit annotations."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
from typing import Sequence

from .schema import DigitSample


FIXED_CROP_NAME = re.compile(
    r"^(?P<source>.+)-(?P<slot>\d+)-(?P<position>hundreds|tens|ones)-[0-9a-f]+\.png$"
)
COMPONENT_CROP_NAME = re.compile(
    r"^(?P<source>.+)-(?P<slot>\d+)-(?P<position>\d+)-[0-9a-f]+\.png$"
)
POSITIONS = ("hundreds", "tens", "ones")


@dataclass(frozen=True)
class SequenceSample:
    sample_id: str
    image: str
    label: str
    source: str
    server: str
    screenshot_type: str
    parent_id: str
    slot: int
    split: str


def sequence_sample_id(parent_id: str, screenshot_type: str, source: str, slot: int) -> str:
    digest = hashlib.sha256(
        f"{parent_id}:{screenshot_type}:{source}:{slot}:sequence-v1".encode("utf-8")
    ).hexdigest()[:16]
    return f"{screenshot_type}-{source}-sequence-{digest}"


def load_verified_sequences(
    path: Path,
    digit_samples: Sequence[DigitSample],
    *,
    screenshot_type: str,
    sources: Sequence[str],
    split_seed: int,
) -> list[SequenceSample]:
    """Use only manually confirmed whole-region labels as CTC targets."""

    if not path.is_file():
        raise ValueError(f"sequence manifest is missing: {path}; run digit-crop and label whole numbers")
    splits_by_parent: dict[str, set[str]] = {}
    for sample in digit_samples:
        if sample.split is not None:
            splits_by_parent.setdefault(sample.parent_id, set()).add(sample.split)
    allowed_sources = set(sources)
    result: list[SequenceSample] = []
    seen: set[str] = set()
    with path.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            raw = json.loads(line)
            sample_id = raw.get("id")
            if not isinstance(sample_id, str) or sample_id in seen:
                raise ValueError(f"{path}:{line_number}: missing or duplicate sequence id")
            seen.add(sample_id)
            label = raw.get("label")
            if label is None or label == "invalid":
                continue
            if not isinstance(label, str) or not re.fullmatch(r"[0-9]{1,12}", label):
                raise ValueError(f"{path}:{line_number}: sequence label must contain 1-12 digits")
            if raw.get("screenshotType") != screenshot_type or raw.get("source") not in allowed_sources:
                continue
            parent_id = raw.get("parentId")
            splits = splits_by_parent.get(parent_id, set())
            if len(splits) > 1:
                raise ValueError(f"{path}:{line_number}: parent appears in multiple splits")
            if splits:
                split = next(iter(splits))
            else:
                # New screenshots can be sequence-labelled before individual digits.
                bucket = int.from_bytes(hashlib.sha256(
                    f"{split_seed}:{parent_id}".encode("utf-8")
                ).digest()[:4], "big") % 10
                split = "train" if bucket < 8 else "validation" if bucket == 8 else "test"
            image = raw.get("image")
            if (not isinstance(image, str) or not image.startswith("regions/")
                    or PurePosixPath(image).is_absolute() or ".." in PurePosixPath(image).parts):
                raise ValueError(f"{path}:{line_number}: invalid sequence image path")
            slot = raw.get("slot")
            if not isinstance(slot, int) or slot < 0:
                raise ValueError(f"{path}:{line_number}: invalid sequence slot")
            if sample_id != sequence_sample_id(parent_id, screenshot_type, raw["source"], slot):
                raise ValueError(f"{path}:{line_number}: sequence identity does not match its source")
            result.append(SequenceSample(
                sample_id=sample_id, image=image, label=label,
                source=raw["source"], server=raw["server"],
                screenshot_type=screenshot_type, parent_id=parent_id,
                slot=slot, split=split,
            ))
    return result


def _fixed_slot_identity(sample: DigitSample) -> tuple[int, str] | None:
    match = FIXED_CROP_NAME.match(PurePosixPath(sample.image).name)
    if match is None or match.group("source") != sample.source:
        return None
    return int(match.group("slot")), match.group("position")


def _sequence_label(by_position: dict[str, DigitSample]) -> str | None:
    if set(by_position) != set(POSITIONS):
        return None
    labels = [by_position[position].label for position in POSITIONS]
    if any(label is None for label in labels):
        return None
    while labels and labels[0] == "invalid":
        labels.pop(0)
    if not labels or any(label == "invalid" for label in labels):
        return None
    if any(label is None or not label.isdigit() for label in labels):
        return None
    return "".join(label for label in labels if label is not None)


def _component_sequence_label(samples: list[tuple[int, DigitSample]]) -> str | None:
    ordered = [sample for _position, sample in sorted(samples)]
    if any(sample.label is None for sample in ordered):
        return None
    digits = [
        sample.label
        for sample in ordered
        if sample.label is not None and sample.label != "invalid"
    ]
    if not digits or any(not label.isdigit() for label in digits):
        return None
    return "".join(digits)


def _region_image(sample: DigitSample, slot: int) -> str:
    crop_path = PurePosixPath(sample.image)
    try:
        crops_index = crop_path.parts.index("crops")
    except ValueError as error:
        raise ValueError(f"unexpected crop path: {sample.image}") from error
    frame_parts = crop_path.parts[crops_index + 1 : -1]
    return str(PurePosixPath("regions", *frame_parts, f"{sample.source}-{slot}.png"))


def _sequence_sample(
    by_position: dict[str, DigitSample] | list[tuple[int, DigitSample]],
    *,
    parent_id: str,
    server: str,
    screenshot_type: str,
    source: str,
    slot: int,
    label: str,
) -> SequenceSample:
    values = (
        list(by_position.values())
        if isinstance(by_position, dict)
        else [sample for _position, sample in by_position]
    )
    splits = {sample.split for sample in values}
    if len(splits) != 1 or None in splits:
        raise ValueError(
            f"crops for {parent_id} {source}:{slot} do not share a split"
        )
    representative = values[-1]
    return SequenceSample(
        sample_id=sequence_sample_id(parent_id, screenshot_type, source, slot),
        image=_region_image(representative, slot),
        label=label,
        source=source,
        server=server,
        screenshot_type=screenshot_type,
        parent_id=parent_id,
        slot=slot,
        split=next(iter(splits)),
    )


def build_sequence_samples(
    samples: Sequence[DigitSample],
    *,
    screenshot_type: str = "battle",
    sources: Sequence[str] | None = None,
) -> list[SequenceSample]:
    allowed_sources = set(sources) if sources is not None else None
    fixed_groups: dict[
        tuple[str, str, str, str, int], dict[str, DigitSample]
    ] = {}
    component_groups: dict[
        tuple[str, str, str, str, int], list[tuple[int, DigitSample]]
    ] = {}
    for sample in samples:
        if sample.screenshot_type != screenshot_type:
            continue
        if allowed_sources is not None and sample.source not in allowed_sources:
            continue
        identity = _fixed_slot_identity(sample)
        if identity is not None:
            slot, position = identity
            key = (
                sample.parent_id,
                sample.server,
                sample.screenshot_type,
                sample.source,
                slot,
            )
            fixed_groups.setdefault(key, {})[position] = sample
            continue

        # NP has retained legacy connected-component crops alongside its fixed
        # slots. They are useful for the single-digit CNN but must not create a
        # second, conflicting sequence target for the same gauge.
        if sample.source == "np_gauge":
            continue
        match = COMPONENT_CROP_NAME.match(PurePosixPath(sample.image).name)
        if match is None or match.group("source") != sample.source:
            continue
        slot = int(match.group("slot"))
        position = int(match.group("position"))
        key = (
            sample.parent_id,
            sample.server,
            sample.screenshot_type,
            sample.source,
            slot,
        )
        component_groups.setdefault(key, []).append((position, sample))

    result: list[SequenceSample] = []
    for (
        parent_id,
        server,
        sample_type,
        source,
        slot,
    ), by_position in sorted(fixed_groups.items()):
        label = _sequence_label(by_position)
        if label is None:
            continue
        result.append(
            _sequence_sample(
                by_position,
                parent_id=parent_id,
                server=server,
                screenshot_type=sample_type,
                source=source,
                slot=slot,
                label=label,
            )
        )
    for (
        parent_id,
        server,
        sample_type,
        source,
        slot,
    ), components in sorted(component_groups.items()):
        label = _component_sequence_label(components)
        if label is None:
            continue
        result.append(
            _sequence_sample(
                components,
                parent_id=parent_id,
                server=server,
                screenshot_type=sample_type,
                source=source,
                slot=slot,
                label=label,
            )
        )
    return result


def validate_sequence_images(dataset_root: Path, samples: Sequence[SequenceSample]) -> None:
    missing = [sample.image for sample in samples if not (dataset_root / sample.image).is_file()]
    if missing:
        raise ValueError(f"sequence crop is missing: {missing[0]}")
