"""Unit tests for mash_cv."""

import json
import os
import subprocess
import sys

import cv2
import numpy as np
import pytest

import mash_cv


@pytest.fixture(autouse=True)
def _clear_templates():
    """Reset global template state between tests."""
    mash_cv.templates.clear()
    yield
    mash_cv.templates.clear()


# ── Helpers ──────────────────────────────────────────────────────────────


def _make_bgr_image(width: int, height: int, bgr=(0, 0, 0)) -> np.ndarray:
    img = np.zeros((height, width, 3), dtype=np.uint8)
    img[:] = bgr
    return img


def _set_pixel_bgr(img: np.ndarray, x_frac: float, y_frac: float, bgr: tuple):
    h, w = img.shape[:2]
    img[int(y_frac * h), int(x_frac * w)] = bgr


def _save_image(img: np.ndarray, path: str):
    cv2.imwrite(path, img)


# ── _pixel_matches ──────────────────────────────────────────────────────


class TestPixelMatches:
    def test_exact_match(self):
        img = _make_bgr_image(100, 100)
        _set_pixel_bgr(img, 0.5, 0.5, (220, 120, 40))  # BGR
        anchor = {"x": 0.5, "y": 0.5, "r": 40, "g": 120, "b": 220, "tol": 0}
        assert mash_cv._pixel_matches(img, anchor) is True

    def test_within_tolerance(self):
        img = _make_bgr_image(100, 100)
        _set_pixel_bgr(img, 0.5, 0.5, (210, 115, 50))
        anchor = {"x": 0.5, "y": 0.5, "r": 40, "g": 120, "b": 220, "tol": 15}
        assert mash_cv._pixel_matches(img, anchor) is True

    def test_outside_tolerance(self):
        img = _make_bgr_image(100, 100)
        _set_pixel_bgr(img, 0.5, 0.5, (100, 100, 100))
        anchor = {"x": 0.5, "y": 0.5, "r": 40, "g": 120, "b": 220, "tol": 10}
        assert mash_cv._pixel_matches(img, anchor) is False

    def test_out_of_bounds(self):
        img = _make_bgr_image(100, 100)
        anchor = {"x": 1.5, "y": 0.5, "r": 0, "g": 0, "b": 0, "tol": 255}
        assert mash_cv._pixel_matches(img, anchor) is False


# ── _detect_screen ──────────────────────────────────────────────────────


class TestDetectScreen:
    def test_team_confirm(self):
        img = _make_bgr_image(100, 100)
        # TeamConfirm anchors: (0.85,0.04) RGB(223,231,236), (0.90,0.93) RGB(207,209,212)
        _set_pixel_bgr(img, 0.85, 0.04, (236, 231, 223))
        _set_pixel_bgr(img, 0.90, 0.93, (212, 209, 207))
        assert mash_cv._detect_screen(img) == "TeamConfirm"

    def test_team_change(self):
        img = _make_bgr_image(100, 100)
        # TeamChange anchors: (0.85,0.04) RGB(53,59,73), (0.05,0.93) RGB(224,86,142)
        _set_pixel_bgr(img, 0.85, 0.04, (73, 59, 53))
        _set_pixel_bgr(img, 0.05, 0.93, (142, 86, 224))
        assert mash_cv._detect_screen(img) == "TeamChange"

    def test_unknown_when_no_match(self):
        img = _make_bgr_image(100, 100)
        assert mash_cv._detect_screen(img) == "Unknown"

    def test_first_match_wins(self):
        """When multiple signatures could match, the first one wins."""
        img = _make_bgr_image(100, 100)
        # Set pixels for both TeamConfirm and TeamChange
        _set_pixel_bgr(img, 0.85, 0.04, (236, 231, 223))
        _set_pixel_bgr(img, 0.90, 0.93, (212, 209, 207))
        _set_pixel_bgr(img, 0.05, 0.93, (142, 86, 224))
        assert mash_cv._detect_screen(img) == "TeamConfirm"


# ── _load_templates ─────────────────────────────────────────────────────


class TestLoadTemplates:
    def test_missing_directory(self):
        result = mash_cv._load_templates("/nonexistent/path")
        assert result["ok"] is False
        assert "not found" in result["error"]

    def test_loads_png_files(self, tmp_path):
        tmpl = np.zeros((20, 30), dtype=np.uint8)
        cv2.imwrite(str(tmp_path / "btn_ok.png"), tmpl)
        cv2.imwrite(str(tmp_path / "btn_cancel.png"), tmpl)

        result = mash_cv._load_templates(str(tmp_path))
        assert result == {"ok": True, "count": 2}
        assert "btn_ok" in mash_cv.templates
        assert "btn_cancel" in mash_cv.templates

    def test_ignores_non_png(self, tmp_path):
        (tmp_path / "readme.txt").write_text("hello")
        tmpl = np.zeros((20, 30), dtype=np.uint8)
        cv2.imwrite(str(tmp_path / "icon.png"), tmpl)

        result = mash_cv._load_templates(str(tmp_path))
        assert result["count"] == 1

    def test_walks_subdirectories(self, tmp_path):
        sub = tmp_path / "sub"
        sub.mkdir()
        tmpl = np.zeros((10, 10), dtype=np.uint8)
        cv2.imwrite(str(sub / "deep.png"), tmpl)

        result = mash_cv._load_templates(str(tmp_path))
        assert result["count"] == 1
        assert "deep" in mash_cv.templates


# ── _find_element ───────────────────────────────────────────────────────


class TestFindElement:
    def test_missing_template_key(self):
        img = _make_bgr_image(100, 100)
        result = mash_cv._find_element(
            img, "nonexistent", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.8
        )
        assert result == {"found": False}

    def test_finds_embedded_patch(self, tmp_path):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        # Paint a gradient patch at (80,80) so it has non-zero variance
        patch_bgr = np.tile(
            np.arange(20, dtype=np.uint8) * 12, (20, 1)
        )  # horizontal gradient
        patch_bgr_3ch = cv2.merge([patch_bgr, patch_bgr, patch_bgr])
        img[80:100, 80:100] = patch_bgr_3ch

        mash_cv.templates["grad"] = patch_bgr.copy()

        result = mash_cv._find_element(
            img, "grad", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.8
        )
        assert result["found"] is True
        assert 0.35 < result["x"] < 0.55
        assert 0.35 < result["y"] < 0.55

    def test_template_larger_than_roi(self):
        img = _make_bgr_image(100, 100)
        big_tmpl = np.zeros((200, 200), dtype=np.uint8)
        mash_cv.templates["big"] = big_tmpl

        result = mash_cv._find_element(
            img, "big", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.8
        )
        assert result == {"found": False}

    def test_respects_region(self):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        # Place a gradient patch in the top-left corner
        patch_gray = np.tile(np.arange(20, dtype=np.uint8) * 12, (20, 1))
        patch_3ch = cv2.merge([patch_gray, patch_gray, patch_gray])
        img[10:30, 10:30] = patch_3ch

        mash_cv.templates["patch"] = patch_gray.copy()

        # Region that excludes the patch (bottom-right quadrant)
        result = mash_cv._find_element(
            img, "patch", {"x": 0.5, "y": 0.5, "w": 0.5, "h": 0.5}, 0.8
        )
        assert result["found"] is False

    def test_below_threshold(self):
        img = _make_bgr_image(200, 200, bgr=(128, 128, 128))
        patch = np.zeros((20, 20), dtype=np.uint8)
        patch[:] = 100  # slightly different gray
        mash_cv.templates["gray"] = patch

        result = mash_cv._find_element(
            img, "gray", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.9999
        )
        assert result["found"] is False

    def test_returns_region_for_match(self):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch_gray = np.tile(np.arange(20, dtype=np.uint8) * 12, (20, 1))
        patch_3ch = cv2.merge([patch_gray, patch_gray, patch_gray])
        img[40:60, 120:140] = patch_3ch
        mash_cv.templates["patch"] = patch_gray

        result = mash_cv._find_element(
            img, "patch", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.8
        )
        assert result["found"] is True
        assert 0.59 <= result["region"]["x"] <= 0.61
        assert 0.19 <= result["region"]["y"] <= 0.21
        assert result["region"]["w"] == pytest.approx(0.1)
        assert result["region"]["h"] == pytest.approx(0.1)


# ── Integration: subprocess REPL ────────────────────────────────────────


class TestREPL:
    """Spin up mash_cv as a subprocess and exercise the JSON-line protocol."""

    def _run(self, commands: list[dict]) -> list[dict]:
        input_text = "\n".join(json.dumps(c) for c in commands) + "\n"
        proc = subprocess.run(
            [sys.executable, "-m", "mash_cv"],
            input=input_text,
            capture_output=True,
            text=True,
            timeout=10,
        )
        lines = [l for l in proc.stdout.strip().splitlines() if l]
        return [json.loads(l) for l in lines]

    def test_quit(self):
        responses = self._run([{"cmd": "quit"}])
        assert responses == []

    def test_unknown_command(self):
        responses = self._run([{"cmd": "nope"}, {"cmd": "quit"}])
        assert len(responses) == 1
        assert "error" in responses[0]

    def test_detect_missing_image(self):
        responses = self._run([
            {"cmd": "detect", "imagePath": "/tmp/__nonexistent__.png"},
            {"cmd": "quit"},
        ])
        assert responses[0]["screen"] == "Unknown"

    def test_load_and_find(self, tmp_path):
        tmpl_dir = tmp_path / "templates"
        tmpl_dir.mkdir()
        patch = np.zeros((20, 20), dtype=np.uint8)
        cv2.imwrite(str(tmpl_dir / "blk.png"), patch)

        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[80:100, 80:100] = (0, 0, 0)
        img_path = str(tmp_path / "scene.png")
        _save_image(img, img_path)

        responses = self._run([
            {"cmd": "load_templates", "dir": str(tmpl_dir)},
            {
                "cmd": "find_element",
                "imagePath": img_path,
                "templateKey": "blk",
                "region": {"x": 0, "y": 0, "w": 1, "h": 1},
                "threshold": 0.8,
            },
            {"cmd": "quit"},
        ])

        assert responses[0] == {"ok": True, "count": 1}
        assert responses[1]["found"] is True

    def test_invalid_json(self):
        proc = subprocess.run(
            [sys.executable, "-m", "mash_cv"],
            input="not json\n{\"cmd\":\"quit\"}\n",
            capture_output=True,
            text=True,
            timeout=10,
        )
        lines = [l for l in proc.stdout.strip().splitlines() if l]
        responses = [json.loads(l) for l in lines]
        assert len(responses) == 1
        assert "error" in responses[0]
        assert "invalid JSON" in responses[0]["error"]

    def test_find_region(self, tmp_path):
        patch_gray = np.tile(np.arange(20, dtype=np.uint8) * 12, (20, 1))
        patch_3ch = cv2.merge([patch_gray, patch_gray, patch_gray])

        img = _make_bgr_image(200, 200, bgr=(180, 180, 180))
        img[80:100, 40:60] = patch_3ch

        img_path = str(tmp_path / "scene.png")
        tmpl_path = str(tmp_path / "tmpl.png")
        _save_image(img, img_path)
        cv2.imwrite(tmpl_path, patch_gray)

        responses = self._run([
            {
                "cmd": "find_region",
                "imagePath": img_path,
                "templatePath": tmpl_path,
                "threshold": 0.8,
            },
            {"cmd": "quit"},
        ])

        assert len(responses) == 1
        assert responses[0]["found"] is True
        assert 0.19 <= responses[0]["region"]["x"] <= 0.21
        assert 0.39 <= responses[0]["region"]["y"] <= 0.41


class TestRegionTool:
    def test_cli_outputs_region(self, tmp_path):
        patch_gray = np.tile(np.arange(20, dtype=np.uint8) * 12, (20, 1))
        patch_3ch = cv2.merge([patch_gray, patch_gray, patch_gray])

        img = _make_bgr_image(200, 200, bgr=(120, 120, 120))
        img[30:50, 60:80] = patch_3ch

        img_path = str(tmp_path / "scene.png")
        tmpl_path = str(tmp_path / "tmpl.png")
        _save_image(img, img_path)
        cv2.imwrite(tmpl_path, patch_gray)

        proc = subprocess.run(
            [
                sys.executable,
                "-m",
                "mash_cv.region_tool",
                "--screenshot",
                img_path,
                "--template",
                tmpl_path,
                "--threshold",
                "0.8",
            ],
            capture_output=True,
            text=True,
            timeout=10,
        )
        result = json.loads(proc.stdout.strip())

        assert proc.returncode == 0
        assert result["found"] is True
        assert 0.29 <= result["region"]["x"] <= 0.31
        assert 0.14 <= result["region"]["y"] <= 0.16
        assert result["originalRegion"] == result["region"]
        assert result["paddedRoi"]["x"] < result["region"]["x"]
        assert result["paddedRoi"]["y"] < result["region"]["y"]

    def test_cli_outputs_padded_roi_with_custom_padding(self, tmp_path):
        patch_gray = np.tile(np.arange(20, dtype=np.uint8) * 12, (20, 1))
        patch_3ch = cv2.merge([patch_gray, patch_gray, patch_gray])

        img = _make_bgr_image(200, 200, bgr=(120, 120, 120))
        img[0:20, 0:20] = patch_3ch

        img_path = str(tmp_path / "scene.png")
        tmpl_path = str(tmp_path / "tmpl.png")
        _save_image(img, img_path)
        cv2.imwrite(tmpl_path, patch_gray)

        proc = subprocess.run(
            [
                sys.executable,
                "-m",
                "mash_cv.region_tool",
                "--screenshot",
                img_path,
                "--template",
                tmpl_path,
                "--padding-x",
                "0.05",
                "--padding-y",
                "0.03",
                "--threshold",
                "0.8",
            ],
            capture_output=True,
            text=True,
            timeout=10,
        )
        result = json.loads(proc.stdout.strip())

        assert proc.returncode == 0
        assert result["found"] is True
        assert result["originalRegion"]["x"] == pytest.approx(0.0)
        assert result["originalRegion"]["y"] == pytest.approx(0.0)
        assert result["originalRegion"]["w"] == pytest.approx(0.1)
        assert result["originalRegion"]["h"] == pytest.approx(0.1)
        assert result["paddedRoi"]["x"] == pytest.approx(0.0)
        assert result["paddedRoi"]["y"] == pytest.approx(0.0)
        assert result["paddedRoi"]["w"] == pytest.approx(0.15)
        assert result["paddedRoi"]["h"] == pytest.approx(0.13)
