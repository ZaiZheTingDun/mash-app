"""Offline evidence tests: pixels, identity and classifier decisions stay aligned."""
import base64
import io
import json

import cv2
import numpy as np
import pytest

from mash_cv import cv
from mash_cv.recognition_diagnostics import RecognitionDiagnostics
from mash_cv.stream import ScrcpyStream


@pytest.fixture
def setup_classifier(monkeypatch):
    random = np.random.default_rng(42)
    template = random.integers(0, 255, (12, 12), dtype=np.uint8)
    monkeypatch.setattr(cv, "templates", {"button": template})
    monkeypatch.setattr(cv, "static_template_keys", set())
    monkeypatch.setattr(cv, "template_dirs", [])
    monkeypatch.setattr(cv, "_template_source_paths", {})
    monkeypatch.setattr(cv, "templates_dir", None)
    region = {"x": 0, "y": 0, "w": 1, "h": 1}
    monkeypatch.setattr(cv, "config", {"screens": {"Battle": {"detect": {
        "template": "button", "region": region, "threshold": .99}}}})
    return template, region


def decode_image(sample):
    return cv2.imdecode(np.frombuffer(base64.b64decode(sample["pngB64"]), np.uint8), cv2.IMREAD_COLOR)


def test_samples_freeze_only_middle_and_last_pixels_and_clear_on_recovery(setup_classifier):
    recorder = RecognitionDiagnostics()
    recorder.reset(6)
    image = np.zeros((50, 80, 3), np.uint8)
    for frame_id in range(1, 7):
        image[:] = frame_id
        recorder.detect(image, {"frameId": frame_id}, cv._detect_screen, ["Battle"])
    image[:] = 200
    samples = recorder.export()["samples"]
    assert [s["stage"] for s in samples] == ["middle", "last"]
    assert [s["frame"]["frameId"] for s in samples] == [3, 6]
    for sample in samples:
        assert np.all(decode_image(sample) == sample["frame"]["frameId"])
    recorder.detect(image, {}, lambda *_: {"screen": "Battle", "score": .9})
    assert list(recorder.samples) == ["last"]
    assert recorder.unknown_count == 0


def test_below_threshold_keeps_score_and_best_box_without_changing_decision(setup_classifier):
    recorder = RecognitionDiagnostics()
    frame = np.random.default_rng(8).integers(0, 255, (50, 80, 3), dtype=np.uint8)
    before = cv._detect_screen(frame, ["Battle"])
    assert recorder.detect(frame, {}, cv._detect_screen, ["Battle"]) == before
    probe = recorder.export()["samples"][-1]["candidates"][0]["probes"][0]
    assert probe["reason"] == "below_threshold"
    assert probe["score"] is not None
    assert probe["bestMatch"]["w"] == 12 / 80
    assert probe["originalTemplateSize"] == {"w": 12, "h": 12}


def test_uncomputable_score_is_null_and_capture_is_automatic(setup_classifier):
    recorder = RecognitionDiagnostics()
    recorder.detect(np.zeros((5, 5, 3), np.uint8), {}, cv._detect_screen, ["Battle"])
    sample = recorder.export()["samples"][-1]
    probe = sample["candidates"][0]["probes"][0]
    assert probe["score"] is None
    assert probe["reason"] == "template_larger_than_region"
    assert "pngB64" in sample
    assert "imageError" not in sample


def test_missing_template_keeps_failure_frame_and_short_circuit_reasons(setup_classifier):
    _, region = setup_classifier
    cv.config["screens"]["Battle"]["detect"] = {"requiredTemplates": [
        {"template": "missing", "region": region}, {"template": "button", "region": region}]}
    recorder = RecognitionDiagnostics()
    recorder.reset()
    frame = np.zeros((50, 80, 3), np.uint8)
    with pytest.raises(cv.TemplateLoadError):
        recorder.detect(frame, {"frameId": 7}, cv._detect_screen, ["Battle"])
    sample = recorder.export()["samples"][-1]
    assert sample["error"]["errorCode"] == "template_load_failed"
    assert [p["reason"] for p in sample["candidates"][0]["probes"]] == ["template_load_failed", "not_evaluated"]
    assert np.array_equal(decode_image(sample), frame)


def test_ordered_alternatives_keep_first_passing_option(setup_classifier):
    template, region = setup_classifier
    cv.config["screens"]["Battle"]["detect"] = {"templateOptions": [
        {"template": "button", "threshold": .8}, {"template": "missing"}], "region": region}
    frame = np.zeros((50, 80, 3), np.uint8)
    frame[5:17, 6:18] = cv2.cvtColor(template, cv2.COLOR_GRAY2BGR)
    recorder = RecognitionDiagnostics()
    assert recorder.detect(frame, {}, cv._detect_screen)["screen"] == "Battle"
    probes = recorder.export()["samples"][-1]["candidates"][0]["probes"]
    assert probes[0]["passed"]
    assert probes[1]["reason"] == "not_evaluated"


def test_stream_snapshot_pairs_pixels_and_metadata_without_a_device():
    stream = ScrcpyStream("unused.jar")
    stream._publish_frame(np.zeros((10, 20, 3), np.uint8))
    image, metadata = stream.get_latest_snapshot()
    stream._publish_frame(np.ones((20, 10, 3), np.uint8))
    assert np.all(image == 0)
    assert metadata["frameId"] == 1
    assert metadata["width"] == 20 and metadata["height"] == 10
    assert stream.get_latest_snapshot()[1]["frameId"] == 2
    assert stream.get_latest_snapshot()[1]["streamId"] == metadata["streamId"]


def test_repl_returns_same_frame_png_and_echoes_request_id(setup_classifier, monkeypatch, tmp_path, capsys):
    frame = np.zeros((50, 80, 3), np.uint8)
    path = tmp_path / "frame.png"
    cv2.imwrite(str(path), frame)
    monkeypatch.setattr(cv, "recognition_diagnostics", RecognitionDiagnostics())
    commands = [{"cmd": "configure_recognition_diagnostics", "captureImages": False, "id": 1},
                {"cmd": "detect", "diagnostics": True, "imagePath": str(path), "id": 2},
                {"cmd": "get_recognition_diagnostics", "id": 3}]
    monkeypatch.setattr(cv.sys, "stdin", io.StringIO("\n".join(json.dumps(c) for c in commands)))
    cv._main_repl()
    responses = [json.loads(line) for line in capsys.readouterr().out.splitlines()]
    assert [r["id"] for r in responses] == [1, 2, 3]
    assert np.array_equal(decode_image(responses[-1]["samples"][-1]), frame)
    assert responses[-1]["configuration"] == cv.config
    assert responses[-1]["runtime"]["pythonVersion"]
    assert responses[-1]["templates"][0]["key"] == "button"


def test_no_frame_does_not_relabel_a_preceding_frame(setup_classifier):
    recorder = RecognitionDiagnostics()
    recorder.reset()
    recorder.detect(np.zeros((50, 80, 3), np.uint8), {"frameId": 1}, cv._detect_screen)
    recorder.no_frame("decoder stopped")
    sample = recorder.export()["samples"][-1]
    assert sample["frame"] is None
    assert sample["error"]["errorCode"] == "frame_unavailable"
    assert "pngB64" not in sample


@pytest.mark.parametrize("width,height,expected_size", [(2560, 1440, 12), (1920, 1080, 9)])
def test_scaled_template_dimensions_are_reported_at_reference_and_1080p(setup_classifier, width, height, expected_size):
    cv.static_template_keys.add("button")
    recorder = RecognitionDiagnostics()
    recorder.detect(np.zeros((height, width, 3), np.uint8), {}, cv._detect_screen)
    probe = recorder.export()["samples"][-1]["candidates"][0]["probes"][0]
    assert probe["originalTemplateSize"] == {"w": 12, "h": 12}
    assert probe["scaledTemplateSize"] == {"w": expected_size, "h": expected_size}
    assert probe["templateReferenceWidth"] == 2560


def test_empty_search_region_has_no_match_score(setup_classifier):
    cv.config["screens"]["Battle"]["detect"]["region"] = {"x": 2, "y": 2, "w": 1, "h": 1}
    recorder = RecognitionDiagnostics()
    recorder.detect(np.zeros((50, 80, 3), np.uint8), {}, cv._detect_screen)
    probe = recorder.export()["samples"][-1]["candidates"][0]["probes"][0]
    assert probe["score"] is None
    assert probe["reason"] == "empty_search_region"


def test_diagnostic_repl_exports_sources_for_shared_template_namespace(setup_classifier, monkeypatch, tmp_path, capsys):
    """A loaded shared/key must map to shared/templates/key.png, not .../shared/key.png."""
    template, region = setup_classifier
    shared = tmp_path / "shared" / "templates"
    server = tmp_path / "cn" / "templates"
    shared.mkdir(parents=True)
    server.mkdir(parents=True)
    for key in ("screen_support_select", "screen_team_party"):
        cv2.imwrite(str(shared / f"{key}.png"), template)
        # Same basename in the server directory must not replace the shared source.
        cv2.imwrite(str(server / f"{key}.png"), 255 - template)
    cv._load_templates(str(shared), key_prefix="shared")
    cv._load_templates(str(server), append=True)
    cv.config["screens"] = {key: {"detect": {"template": f"shared/{key}", "region": region}}
                            for key in ("screen_support_select", "screen_team_party")}
    frame_path = tmp_path / "frame.png"
    cv2.imwrite(str(frame_path), np.zeros((50, 80, 3), np.uint8))
    monkeypatch.setattr(cv, "recognition_diagnostics", RecognitionDiagnostics())
    commands = [{"cmd": "configure_recognition_diagnostics", "id": 1},
                {"cmd": "detect", "imagePath": str(frame_path), "diagnostics": True, "id": 2},
                {"cmd": "get_recognition_diagnostics", "id": 3}]
    monkeypatch.setattr(cv.sys, "stdin", io.StringIO("\n".join(json.dumps(c) for c in commands)))
    cv._main_repl()
    report = [json.loads(line) for line in capsys.readouterr().out.splitlines()][-1]
    assert {entry["key"]: entry["sourcePath"] for entry in report["templates"]} == {
        f"shared/{key}": str(shared / f"{key}.png")
        for key in ("screen_support_select", "screen_team_party")
    }


def test_template_sources_follow_namespace_override_and_reload(setup_classifier, tmp_path):
    template, _ = setup_classifier
    first = tmp_path / "first"
    override = tmp_path / "override"
    replacement = tmp_path / "replacement"
    for directory in (first, override, replacement):
        directory.mkdir()
        cv2.imwrite(str(directory / "button.png"), template)
    cv._load_templates(str(first), key_prefix="shared")
    cv._load_templates(str(override), append=True, key_prefix="shared")
    assert cv._template_source_path_for_key("shared/button") == str(override / "button.png")
    cv._load_templates(str(replacement))
    assert cv._template_source_path_for_key("shared/button") is None
    assert cv._template_source_path_for_key("button") == str(replacement / "button.png")


@pytest.mark.parametrize("timeout_count", [1, 2, 3, 6, 100])
def test_no_initial_sample_and_at_most_two_images_at_any_timeout(setup_classifier, timeout_count):
    recorder = RecognitionDiagnostics()
    recorder.reset(timeout_count)
    frame = np.zeros((50, 80, 3), np.uint8)
    for index in range(timeout_count):
        recorder.detect(frame, {"frameId": index + 1}, cv._detect_screen)
        assert "first" not in recorder.samples
        assert len(recorder.samples) <= 2
    samples = recorder.export()["samples"]
    assert len(samples) <= 2
    assert all("pngB64" in sample for sample in samples)
    assert all(sample["frame"]["frameId"] != 1 for sample in samples if sample["stage"] == "middle")
