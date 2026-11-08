"""Conservative template fallback for Japanese support NP-level labels."""

import cv2
import numpy as np


REFERENCE_WIDTH = 1920
MIN_SCORE = 0.78
MIN_MARGIN = 0.04


def _outline_mask(image: np.ndarray) -> np.ndarray:
    gray = cv2.cvtColor(image, cv2.COLOR_BGR2GRAY) if image.ndim == 3 else image
    # Both white and strengthened yellow labels have the same dark outline.
    mask = np.clip((100.0 - gray.astype(np.float32)) * 4.0, 0, 255).astype(np.uint8)
    return cv2.GaussianBlur(mask, (3, 3), 0.6)


def match_np_level(
    crop: np.ndarray, refs: list[tuple[int, np.ndarray]], frame_width: int
) -> int | None:
    """Match the whole Lv.N label, scaling 1080p refs to the current frame.

    Retaining the prefix's *shape* prevents digits inside NP names from
    becoming levels; this path does not invoke OCR or parse prefix text.
    """
    if crop.size == 0 or frame_width <= 0:
        return None
    mask = _outline_mask(crop)
    if not np.any(mask):
        return None
    scores = []
    for level, ref in refs:
        best_score = 0.0
        reference = ref
        for adjustment in np.linspace(0.85, 1.15, 13):
            scale = frame_width / REFERENCE_WIDTH * adjustment
            tw = max(1, round(reference.shape[1] * scale))
            th = max(1, round(reference.shape[0] * scale))
            if tw > mask.shape[1] or th > mask.shape[0] or th < 8:
                continue
            scaled = _outline_mask(cv2.resize(reference, (tw, th), interpolation=cv2.INTER_AREA))
            result = cv2.matchTemplate(mask, scaled, cv2.TM_CCOEFF_NORMED)
            _, score, _, (x, y) = cv2.minMaxLoc(result)
            # A neighbouring digit would turn Lv.1 into Lv.10 / Lv.15.
            # The reference includes a small blank pad after its last digit.
            right = x + tw + 1
            tail = mask[y:y + th, right:min(mask.shape[1], x + tw + round(th * 0.3))]
            if tail.size:
                _, _, stats, _ = cv2.connectedComponentsWithStats(tail)
                if any(h >= th * 0.5 and area >= th * 0.3 for _, _, _, h, area in stats[1:]):
                    continue
            best_score = max(best_score, float(score))
        scores.append((best_score, level))
    scores.sort(reverse=True)
    if len(scores) < 2:
        return None
    score, level = scores[0]
    return level if score >= MIN_SCORE and score - scores[1][0] >= MIN_MARGIN else None
