"""Bounded, in-memory evidence for screen detection; never changes decisions.

Only the detect REPL endpoint activates this collector. Images are frozen before
classification and encoded on export, so a later video frame cannot replace them.
"""
from __future__ import annotations

import base64
import copy
from contextvars import ContextVar
import math
import time
import sys
from typing import Callable

import cv2

_active: ContextVar[dict | None] = ContextVar("recognition_diagnostics", default=None)


def begin_candidate(name: str, spec: dict) -> None:
    collector = _active.get()
    if collector is not None:
        candidate = {"screen": name, "priority": spec.get("priority", 0),
                     "rule": copy.deepcopy(spec), "probes": []}
        collector["candidates"].append(candidate)
        collector["current"] = candidate


def record_match(result: dict, template_key, original_size, scaled_size,
                 search_region, threshold, reference_width=None, masked=False) -> dict:
    collector = _active.get()
    if collector is not None and collector.get("current") is not None:
        reason = result.get("failReason")
        if not reason and not result.get("found"):
            reason = "below_threshold"
        score = result.get("score")
        collector["current"]["probes"].append({
            "template": template_key, "score": score if not result.get("failReason") and score is not None and math.isfinite(score) else None,
            "threshold": threshold, "passed": bool(result.get("found")),
            "reason": reason, "searchRegion": copy.deepcopy(search_region),
            "bestMatch": result.get("region"), "originalTemplateSize": original_size,
            "scaledTemplateSize": scaled_size, "templateReferenceWidth": reference_width,
            "masked": masked,
        })
    return result


def _declared_probes(rule: dict) -> list[str]:
    if rule.get("templateOptions"):
        return [str(p.get("template")) for p in rule["templateOptions"] if isinstance(p, dict) and p.get("template")]
    if rule.get("requiredTemplates"):
        return [str(p.get("template")) for p in rule["requiredTemplates"] if isinstance(p, dict) and p.get("template")]
    return [str(p) for p in rule.get("templates", [rule.get("template")]) if p]


class RecognitionDiagnostics:
    def __init__(self) -> None:
        self.reset()

    def reset(self, timeout_count: int = 100) -> None:
        self.midpoint = max(2, timeout_count // 2)
        self.unknown_count = 0
        self.samples: dict[str, tuple[dict, object]] = {}

    def detect(self, img, metadata: dict, classify: Callable, screen_names=None) -> dict:
        collector = {"candidates": [], "current": None}
        token = _active.set(collector)
        result = None
        error = None
        try:
            result = classify(img, screen_names)
            return result
        except Exception as exc:
            error = getattr(exc, "response", {"error": str(exc), "errorCode": "classification_failed"})
            raise
        finally:
            _active.reset(token)
            for candidate in collector["candidates"]:
                probes = candidate["probes"]
                # Preserve ordered alternatives / required-template short circuits.
                for key in _declared_probes(candidate["rule"])[len(probes):]:
                    probes.append({"template": key, "score": None, "passed": False,
                                   "reason": "template_load_failed" if error and key == error.get("templateKey") else "not_evaluated"})
            if result and result.get("screen") != "Unknown":
                self.samples.clear()
                self.unknown_count = 0
            else:
                self.unknown_count += 1
            evidence = {"frame": copy.deepcopy(metadata), "observedAtMs": int(time.time() * 1000),
                        "result": result, "error": error,
                        "screenScope": copy.deepcopy(screen_names),
                        "unknownCount": self.unknown_count, "candidates": collector["candidates"]}
            frozen = img.copy() if img is not None else None
            sample = (evidence, frozen)
            if self.unknown_count == self.midpoint:
                self.samples["middle"] = sample
            # Also retain the last successful classification for other stop paths.
            self.samples["last"] = sample

    def no_frame(self, error: str) -> None:
        # Never present a preceding frame as the input of a failed acquisition.
        self.samples["last"] = ({"frame": None, "result": None,
                                 "error": {"errorCode": "frame_unavailable", "error": error},
                                 "candidates": [], "observedAtMs": int(time.time() * 1000)}, None)

    def export(self, configuration=None, template_path=None) -> dict:
        samples = []
        for stage in ("middle", "last"):
            if stage not in self.samples:
                continue
            evidence, image = self.samples[stage]
            entry = {**evidence, "stage": stage}
            if image is not None:
                try:
                    ok, encoded = cv2.imencode(".png", image)
                    if ok:
                        entry["pngB64"] = base64.b64encode(encoded).decode("ascii")
                    else:
                        entry["imageError"] = "png_encode_failed"
                except Exception:
                    entry["imageError"] = "png_encode_failed"
            else:
                entry["imageError"] = "frame_unavailable"
            samples.append(entry)
        report = {"schemaVersion": 1, "samples": samples,
                  "runtime": {"pythonVersion": sys.version.split()[0], "opencvVersion": cv2.__version__}}
        if configuration is not None:
            report["configuration"] = configuration
        if template_path is not None:
            keys = {probe["template"] for sample in samples
                    for candidate in sample["candidates"] for probe in candidate["probes"]
                    if probe.get("template")}
            report["templates"] = [{"key": key, "sourcePath": template_path(key)} for key in sorted(keys)]
        return report
