from dataclasses import replace
import json

import cv2
import numpy as np

from digit_training.crop import (
    SCREENSHOT_DEFINITIONS,
    NP_GAUGE_DIGIT_SLOTS,
    NP_GAUGE_SOURCE_REGIONS,
    SourceRegion,
    _component_boxes,
    _fixed_digit_region,
    _pixel_region,
    extract_dataset,
)
from digit_training.schema import load_manifest, save_manifest


def test_component_boxes_find_digit_sized_bright_glyphs_left_to_right():
    image = np.zeros((1080, 1920, 3), dtype=np.uint8)
    cv2.putText(
        image,
        "12",
        (1320, 50),
        cv2.FONT_HERSHEY_SIMPLEX,
        1.0,
        (255, 255, 255),
        2,
        cv2.LINE_8,
    )
    region = SourceRegion("battle_progress", 0, 0.683, 0.012, 0.050, 0.040)

    boxes = _component_boxes(image, region)

    assert len(boxes) == 2
    assert boxes[0][0] < boxes[1][0]


def test_shared_config_contains_all_battle_sources_and_separate_ctc_regions():
    battle = SCREENSHOT_DEFINITIONS["battle"]
    sources = {source.name: source for source in battle.sources}

    assert set(sources) == {
        "enemy_hp",
        "ally_hp",
        "np_gauge",
        "battle_progress",
        "enemy_count",
        "turn_count",
    }
    assert sources["np_gauge"].regions[1].x == 0.429
    assert sources["np_gauge"].sequence_regions[1].x == 0.430
    assert all(region.width == 0.0308 for region in sources["np_gauge"].sequence_regions)
    assert all(region.width == 0.0297 for region in sources["np_gauge"].regions)


def test_extract_dataset_writes_manifest_and_preserves_existing_labels(tmp_path):
    dataset_root = tmp_path / "data"
    raw_dir = dataset_root / "raw" / "cn"
    raw_dir.mkdir(parents=True)
    image = np.zeros((1080, 1920, 3), dtype=np.uint8)
    cv2.putText(
        image,
        "12",
        (1320, 50),
        cv2.FONT_HERSHEY_SIMPLEX,
        1.0,
        (255, 255, 255),
        2,
        cv2.LINE_8,
    )
    screenshot = raw_dir / "frame.png"
    assert cv2.imwrite(str(screenshot), image)

    first = extract_dataset(dataset_root)
    manifest_path = dataset_root / "manifest.jsonl"
    samples = load_manifest(manifest_path)
    sequence_path = dataset_root / "sequence-manifest.jsonl"
    sequences = [json.loads(line) for line in sequence_path.read_text().splitlines()]

    assert first["screenshots"] == 1
    assert samples
    assert sequences
    assert all((dataset_root / row["image"]).is_file() for row in sequences)
    sequences[0]["label"] = "12"
    sequence_path.write_text("\n".join(json.dumps(row) for row in sequences) + "\n")
    assert all(sample.parent_id.startswith("sha256:") for sample in samples)
    assert all(sample.server == "cn" for sample in samples)
    labeled = replace(samples[0], label="1", notes="checked")
    save_manifest(manifest_path, [labeled, *samples[1:]])

    second = extract_dataset(dataset_root)
    regenerated = {sample.sample_id: sample for sample in load_manifest(manifest_path)}
    regenerated_sequences = {
        row["id"]: row for row in map(json.loads, sequence_path.read_text().splitlines())
    }

    assert second["preservedLabels"] == 1
    assert regenerated[labeled.sample_id].label == "1"
    assert regenerated[labeled.sample_id].notes == "checked"
    assert regenerated_sequences[sequences[0]["id"]]["label"] == "12"
    assert first["screenshotTypes"] == {"battle": 1}
    assert all(sample.screenshot_type == "battle" for sample in samples)
    assert (dataset_root / "previews" / "battle" / "cn" / "frame.png").exists()


def test_extract_dataset_uses_fixed_np_slots_from_shared_config(tmp_path):
    dataset_root = tmp_path / "data"
    raw_dir = dataset_root / "raw" / "cn"
    raw_dir.mkdir(parents=True)
    image = np.zeros((1080, 1920, 3), dtype=np.uint8)
    screenshot = raw_dir / "frame.png"
    assert cv2.imwrite(str(screenshot), image)

    extract_dataset(dataset_root)
    np_samples = [
        sample
        for sample in load_manifest(dataset_root / "manifest.jsonl")
        if sample.source == "np_gauge"
    ]

    assert len(np_samples) == 9
    assert {sample.style for sample in np_samples} == {"battle_hud_fixed_slot"}
    assert [
        sum(position in sample.sample_id for sample in np_samples)
        for position in ("hundreds", "tens", "ones")
    ] == [3, 3, 3]
    expected = _pixel_region(
        _fixed_digit_region(NP_GAUGE_SOURCE_REGIONS[0], NP_GAUGE_DIGIT_SLOTS[0]),
        1920,
        1080,
    )
    first_hundreds = next(
        sample
        for sample in np_samples
        if "hundreds" in sample.sample_id
        and sample.source_bbox_px == expected
    )
    crop = cv2.imread(str(dataset_root / first_hundreds.image), cv2.IMREAD_COLOR)
    assert crop.shape[:2] == (expected[3], expected[2])


def test_extract_dataset_preserves_labeled_legacy_np_component_samples(tmp_path):
    dataset_root = tmp_path / "data"
    raw_dir = dataset_root / "raw" / "cn"
    raw_dir.mkdir(parents=True)
    image = np.zeros((1080, 1920, 3), dtype=np.uint8)
    assert cv2.imwrite(str(raw_dir / "frame.png"), image)

    extract_dataset(dataset_root)
    manifest_path = dataset_root / "manifest.jsonl"
    samples = load_manifest(manifest_path)
    legacy_path = (
        dataset_root / "crops" / "battle" / "cn" / "frame" / "legacy-np.png"
    )
    assert cv2.imwrite(str(legacy_path), np.full((30, 12, 3), 255, dtype=np.uint8))
    legacy = replace(
        next(sample for sample in samples if sample.source == "np_gauge"),
        sample_id="battle-np_gauge-legacy-component",
        image="crops/battle/cn/frame/legacy-np.png",
        style="battle_hud",
        label="1",
        notes="legacy component",
    )
    save_manifest(manifest_path, [*samples, legacy])

    result = extract_dataset(dataset_root)
    regenerated = {sample.sample_id: sample for sample in load_manifest(manifest_path)}

    assert result["preservedLegacyNpSamples"] == 1
    assert regenerated[legacy.sample_id].label == "1"
    assert regenerated[legacy.sample_id].notes == "legacy component"


def test_extract_dataset_reads_canonical_screenshot_type_directory(tmp_path):
    dataset_root = tmp_path / "data"
    raw_dir = dataset_root / "raw" / "battle" / "jp"
    raw_dir.mkdir(parents=True)
    assert cv2.imwrite(
        str(raw_dir / "frame.png"), np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    result = extract_dataset(dataset_root)
    samples = load_manifest(dataset_root / "manifest.jsonl")

    assert result["screenshotTypes"] == {"battle": 1}
    assert {sample.screenshot_type for sample in samples} == {"battle"}
    assert {sample.server for sample in samples} == {"jp"}
    assert all(sample.image.startswith("crops/battle/jp/") for sample in samples)
