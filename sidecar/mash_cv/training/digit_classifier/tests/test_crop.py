from dataclasses import replace

import cv2
import numpy as np

from digit_training.crop import SourceRegion, _component_boxes, extract_dataset
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

    assert first["screenshots"] == 1
    assert samples
    assert all(sample.parent_id.startswith("sha256:") for sample in samples)
    assert all(sample.server == "cn" for sample in samples)
    labeled = replace(samples[0], label="1", notes="checked")
    save_manifest(manifest_path, [labeled, *samples[1:]])

    second = extract_dataset(dataset_root)
    regenerated = {sample.sample_id: sample for sample in load_manifest(manifest_path)}

    assert second["preservedLabels"] == 1
    assert regenerated[labeled.sample_id].label == "1"
    assert regenerated[labeled.sample_id].notes == "checked"
    assert (dataset_root / "previews" / "cn" / "frame.png").exists()
