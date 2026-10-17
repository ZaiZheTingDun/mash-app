import numpy as np

from digit_training.train_ctc import (
    BLANK_INDEX,
    greedy_decode,
    normalize_sequence_crop,
)


def test_normalize_sequence_crop_preserves_shape_and_foreground():
    image = np.zeros((30, 58, 3), dtype=np.uint8)
    image[4:26, 10:18] = 255

    normalized = normalize_sequence_crop(image)

    assert normalized.shape == (32, 96)
    assert normalized.dtype == np.float32
    assert normalized.max() == 1.0
    assert normalized[:, :10].max() == 0.0


def test_greedy_decode_collapses_repeats_and_blanks():
    logits = np.full((7, 11), -10.0, dtype=np.float32)
    for time, index in enumerate((1, 1, BLANK_INDEX, 1, BLANK_INDEX, 0, 0)):
        logits[time, index] = 10.0

    label, confidence = greedy_decode(logits)

    assert label == "110"
    assert confidence > 0.99
