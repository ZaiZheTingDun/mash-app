"""Match expected party artwork in fixed slots on one confirmation frame.

Party portraits use narrow_servant artwork scaled to card width. An interior
upper-body crop avoids class, rarity and bonus overlays. Party CE strips use
full card_ce artwork; support-list CE cropping does not apply here.
"""
from pathlib import Path

import cv2
import numpy as np

from mash_cv.host.io import read_image

# Normalized against the recorded 2560 x 1440 confirmation page.
SLOT_X = 0.030
# The back line starts after a wider front/back separator, not three pitches.
BACKLINE_SLOT_X = 0.512
SLOT_PITCH = 0.157
SLOT_WIDTH = 0.147
SERVANT_REGION_Y = 0.29
SERVANT_REGION_BOTTOM = 0.525
CE_REGION_Y = 0.603
CE_REGION_BOTTOM = 0.726
SERVANT_THRESHOLD = 0.80
CE_THRESHOLD = 0.75


def _slot_x(slot: int, frame_width: int) -> int:
    row_start = SLOT_X if slot < 3 else BACKLINE_SLOT_X
    return round((row_start + SLOT_PITCH * (slot % 3)) * frame_width)


def _artwork_score(img: np.ndarray, path: str, slot: int, kind: str) -> float:
    raw = read_image(path, cv2.IMREAD_GRAYSCALE)
    if raw is None:
        raise ValueError("图片无法读取，请更新资源")
    h, w = img.shape[:2]
    card_width = max(1, round(SLOT_WIDTH * w))
    x = _slot_x(slot, w)
    if kind == "servant":
        size = (card_width, max(1, round(raw.shape[0] * card_width / raw.shape[1])))
        scaled = cv2.resize(raw, size, interpolation=cv2.INTER_AREA)
        th = scaled.shape[0]
        template = scaled[
            round(.16 * th):round(.43 * th),
            round(.2 * card_width):round(.85 * card_width),
        ]
        top, bottom = SERVANT_REGION_Y, SERVANT_REGION_BOTTOM
    else:
        size = (card_width, max(1, round(card_width * 68 / 150)))
        scaled = cv2.resize(raw, size, interpolation=cv2.INTER_AREA)
        th = scaled.shape[0]
        template = scaled[
            round(.24 * th):round(.72 * th),
            round(.12 * card_width):round(.88 * card_width),
        ]
        top, bottom = CE_REGION_Y, CE_REGION_BOTTOM
    roi = img[round(top * h):round(bottom * h), x:x + card_width]
    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY) if roi.ndim == 3 else roi
    if template.size == 0 or gray.shape[0] < template.shape[0] or gray.shape[1] < template.shape[1]:
        raise ValueError("画面中的编队区域不可用")
    # Constant templates have no identity signal; normalized correlation would
    # otherwise report 1.0 even against unrelated artwork.
    if float(template.std()) < 2.0:
        return 0.0
    scores = cv2.matchTemplate(gray, template, cv2.TM_CCOEFF_NORMED)
    finite = scores[np.isfinite(scores)]
    return float(finite.max()) if finite.size else 0.0


def verify_team_slots(img: np.ndarray, targets: list[dict]) -> dict:
    issues, checks = [], []
    for target in targets:
        slot, kind = int(target["slotIndex"]), target["kind"]
        if slot not in range(6) or kind not in ("servant", "craftEssence"):
            raise ValueError("无效的编队检查目标")
        paths = target.get("templatePaths") or []
        threshold = SERVANT_THRESHOLD if kind == "servant" else CE_THRESHOLD
        best, errors = 0.0, []
        best_template = None
        if not paths:
            reason = "缺少识别图片，请更新资源"
        else:
            for path in paths:
                try:
                    if not Path(path).is_file():
                        raise ValueError("缺少识别图片，请更新资源")
                    score = _artwork_score(img, path, slot, kind)
                    if score > best:
                        best, best_template = score, Path(path).name
                except (ValueError, cv2.error, OSError) as error:
                    errors.append(str(error))
            reason = errors[0] if errors and best < threshold else "未匹配到预期对象"
        passed = best >= threshold
        checks.append({
            "slotIndex": slot, "kind": kind, "score": best,
            "threshold": threshold, "passed": passed,
            "bestTemplate": best_template, "templateCount": len(paths),
        })
        if not passed:
            issues.append({
                "slotIndex": slot, "kind": kind,
                "expectedName": target["expectedName"], "reason": reason,
            })
    h, w = img.shape[:2]
    return {"issues": issues, "checks": checks, "frameSize": {"w": w, "h": h}}
