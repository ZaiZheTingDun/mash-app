import io
from pathlib import Path
from types import SimpleNamespace

import numpy as np

from mash_cv import cv
from mash_cv import ocr_client as ocr_client_module
from mash_cv.ocr_client import OcrWorkerClient, _prepare_images
from mash_cv.ocr_protocol import read_message, write_message
from mash_cv.ocr_worker import _decode_images, serve


class _FakeEngine:
    def __init__(self, server: str) -> None:
        self.server = server

    def detect(self, image: np.ndarray) -> list[list]:
        return [[[[0, 0], [2, 0], [2, 1], [0, 1]], self.server, float(image.mean())]]

    def recognize_batch(self, images: list[np.ndarray]) -> list[list]:
        return [[f"{self.server}:{image.shape[1]}", 0.9] for image in images]


def _request_bytes(requests: list[tuple[dict, list[np.ndarray]]]) -> io.BytesIO:
    stream = io.BytesIO()
    for header, images in requests:
        prepared = _prepare_images(images)
        write_message(
            stream,
            {**header, "images": prepared.descriptors},
            prepared.payload_parts,
        )
    stream.seek(0)
    return stream


def test_binary_protocol_round_trips_multiple_images() -> None:
    images = [
        np.arange(18, dtype=np.uint8).reshape(2, 3, 3),
        np.full((4, 5), 7, dtype=np.uint8),
    ]
    prepared = _prepare_images(images)
    wire = io.BytesIO()
    write_message(wire, {"images": prepared.descriptors}, prepared.payload_parts)
    wire.seek(0)

    header, payload = read_message(wire)
    decoded = _decode_images(header, payload)

    assert len(decoded) == 2
    assert np.array_equal(decoded[0], images[0])
    assert np.array_equal(decoded[1], images[1])


def test_worker_serves_detect_recognize_and_quit() -> None:
    reader = _request_bytes(
        [
            (
                {"id": 1, "action": "detect", "server": "JP"},
                [np.full((2, 3, 3), 4, dtype=np.uint8)],
            ),
            (
                {"id": 2, "action": "recognize_batch", "server": "JP"},
                [
                    np.zeros((2, 5, 3), dtype=np.uint8),
                    np.zeros((2, 7, 3), dtype=np.uint8),
                ],
            ),
            ({"id": 3, "action": "quit"}, []),
        ]
    )
    writer = io.BytesIO()

    serve(reader, writer, engine_factory=_FakeEngine)
    writer.seek(0)

    detect, payload = read_message(writer)
    assert payload == b""
    assert detect["id"] == 1
    assert detect["ok"] is True
    assert detect["result"][0][1] == "JP"

    recognize, payload = read_message(writer)
    assert payload == b""
    assert recognize["result"] == [["JP:5", 0.9], ["JP:7", 0.9]]

    quit_response, payload = read_message(writer)
    assert payload == b""
    assert quit_response == {"id": 3, "ok": True}


def test_worker_rebuilds_engine_when_server_changes() -> None:
    created: list[str] = []

    def factory(server: str) -> _FakeEngine:
        created.append(server)
        return _FakeEngine(server)

    reader = _request_bytes(
        [
            ({"id": 1, "action": "recognize_batch", "server": "JP"}, [np.zeros((1, 2), dtype=np.uint8)]),
            ({"id": 2, "action": "recognize_batch", "server": "CN"}, [np.zeros((1, 2), dtype=np.uint8)]),
            ({"id": 3, "action": "quit"}, []),
        ]
    )
    writer = io.BytesIO()

    serve(reader, writer, engine_factory=factory)

    assert created == ["JP", "CN"]


def test_full_list_ocr_reuses_current_support_search_worker() -> None:
    class Ocr:
        calls = 0

        def __call__(self, _image):
            self.calls += 1
            return [], 0.0

    ocr = Ocr()

    assert cv._support_full_list_ocr(ocr, np.zeros((2, 2, 3), dtype=np.uint8)) == (
        [],
        0.0,
    )
    assert ocr.calls == 1


def test_client_reuses_worker_until_explicit_release(monkeypatch) -> None:
    workers = []

    class Worker:
        def __init__(self, generation: int) -> None:
            self.generation = generation
            self.process = SimpleNamespace(pid=9000 + generation)
            self.closed = False
            workers.append(self)

        def request(self, header, _payload_parts, _timeout):
            return {"id": header["id"], "ok": True, "result": [], "elapsed": 0.0}

        def close(self):
            self.closed = True

    monkeypatch.setattr(ocr_client_module, "_WorkerProcess", Worker)
    client = OcrWorkerClient("JP")

    client(np.zeros((2, 2, 3), dtype=np.uint8))
    client.text_recognizer([np.zeros((1, 3, 3), dtype=np.uint8)])

    assert len(workers) == 1
    assert workers[0].closed is False
    client.close("support-search-complete")
    assert workers[0].closed is True


def test_client_retries_once_with_a_fresh_worker(monkeypatch) -> None:
    workers = []
    request_timeouts = []

    class Worker:
        def __init__(self, generation: int) -> None:
            self.generation = generation
            self.process = SimpleNamespace(pid=1000 + generation)
            self.closed = False
            workers.append(self)

        def request(self, header, _payload_parts, timeout):
            action = header["action"]
            if action in ("ping", "quit"):
                return {"id": header["id"], "ok": True}
            request_timeouts.append(timeout)
            if self.generation == 1:
                raise RuntimeError("simulated worker crash")
            return {"id": header["id"], "ok": True, "result": [], "elapsed": 0.0}

        def close(self):
            self.closed = True

    monkeypatch.setattr(ocr_client_module, "_WorkerProcess", Worker)
    client = OcrWorkerClient("JP")

    result, _elapsed = client(np.zeros((2, 2, 3), dtype=np.uint8))

    assert result == []
    assert len(workers) == 2
    assert request_timeouts == [ocr_client_module.OCR_WORKER_REQUEST_TIMEOUT_SECONDS] * 2
    assert workers[0].closed is True
    client.close()


def test_client_recycles_immediately_after_large_detector_request(monkeypatch) -> None:
    workers = []

    class Worker:
        def __init__(self, generation: int) -> None:
            self.generation = generation
            self.process = SimpleNamespace(pid=2000 + generation)
            self.closed = False
            workers.append(self)

        def request(self, header, _payload_parts, _timeout):
            return {"id": header["id"], "ok": True, "result": [], "elapsed": 0.0}

        def close(self):
            self.closed = True

    monkeypatch.setattr(ocr_client_module, "_WorkerProcess", Worker)
    client = OcrWorkerClient("JP")

    client.detect(np.zeros((2, 2, 3), dtype=np.uint8), recycle_after=True)

    assert workers[0].closed is True
    assert client._worker is None


def test_development_models_live_in_sibling_runtime_directory(tmp_path, monkeypatch):
    from mash_cv import ocr_worker

    package = tmp_path / "cv-code" / "mash_cv"
    package.mkdir(parents=True)
    models = tmp_path / "cv-runtime" / "models"
    models.mkdir(parents=True)
    monkeypatch.setattr(ocr_worker, "__file__", str(package / "ocr_worker.py"))
    monkeypatch.delenv("MASH_CV_MODELS_DIR", raising=False)
    assert Path(ocr_worker._models_dir()).resolve() == models

    installed_models = tmp_path / "installed-models"
    installed_models.mkdir()
    monkeypatch.setenv("MASH_CV_MODELS_DIR", str(installed_models))
    assert ocr_worker._models_dir() == str(installed_models)


def test_v6_adapter_uses_shared_local_models_and_preserves_wire_results(monkeypatch):
    import rapidocr
    from mash_cv.ocr_worker import RapidOcrEngine

    calls = []
    result = SimpleNamespace(
        boxes=np.array([[[0, 0], [2, 0], [2, 1], [0, 1]]]),
        txts=("文字",), scores=[np.float32(0.9)],
    )

    class Engine:
        def __init__(self, params):
            calls.append(params)

        def _load_rec_model(self):
            def recognize(request):
                assert len(request.img) == 2
                return SimpleNamespace(txts=("中文", "日本語"), scores=[0.8, 0.9])
            return recognize

        def __call__(self, image):
            return result

    monkeypatch.setattr(rapidocr, "RapidOCR", Engine)
    cn, jp = RapidOcrEngine("CN"), RapidOcrEngine("JP")
    assert calls[0] == calls[1]
    assert Path(calls[0]["Rec.model_path"]).name == "PP-OCRv6_rec_small.onnx"
    assert calls[0]["EngineConfig.onnxruntime.intra_op_num_threads"] == 1
    images = [np.zeros((8, 16, 3), dtype=np.uint8)] * 2
    assert cn.recognize_batch(images) == [["中文", 0.8], ["日本語", 0.9]]
    assert jp.recognize_batch([]) == []
    assert cn.detect(images[0])[0][1] == "文字"
    # numpy scalar scores/boxes must be JSON serializable through worker IPC.
    import json
    json.dumps(cn.detect(images[0]))
    result.boxes = None
    result.txts = None
    assert cn.detect(images[0]) == []


def test_v6_missing_models_fail_without_downloading_or_silent_fallback(tmp_path, monkeypatch):
    import pytest
    import rapidocr
    from mash_cv import ocr_worker

    monkeypatch.setattr(ocr_worker, "_models_dir", lambda: str(tmp_path))
    def unexpected(*args, **kwargs):
        raise AssertionError("must validate local models before constructing RapidOCR")
    monkeypatch.setattr(rapidocr, "RapidOCR", unexpected)
    with pytest.raises(FileNotFoundError, match="matching CV runtime"):
        ocr_worker.RapidOcrEngine("CN")


def test_v6_real_worker_recognizes_cn_jp_batches_and_detects_without_network(monkeypatch):
    import cv2
    import requests

    def unexpected(*args, **kwargs):
        raise AssertionError("bundled OCR must work offline")
    monkeypatch.setattr(requests.sessions.Session, "request", unexpected)
    # Existing real screenshots, manually checked names. Use both languages
    # in the same batch to pin multilingual weights and output ordering.
    fixtures = Path(__file__).parent / "test_data"
    cases = [
        (fixtures / "support/debug_1_skill_5_10_4.png", (542, 436, 963, 482), "伊斯坎达尔"),
        (fixtures / "screenshots/support_score_grand_jp_14_16_62_16.png",
         (949, 560, 1628, 625), "ジャンヌ・ダルク〔オルタ〕"),
    ]
    images = []
    for path, (x0, y0, x1, y1), _text in cases:
        frame = cv2.imread(str(path))
        images.append(frame[y0:y1, x0:x1])
    writer = io.BytesIO()
    serve(_request_bytes([
        ({"id": 1, "action": "recognize_batch", "server": "CN"}, images),
        ({"id": 2, "action": "recognize_batch", "server": "JP"}, images),
        ({"id": 3, "action": "detect", "server": "JP"}, [images[1]]),
        ({"id": 4, "action": "quit"}, []),
    ]), writer)
    writer.seek(0)
    for request_id in (1, 2):
        response, _ = read_message(writer)
        assert response["ok"], response
        assert response["id"] == request_id
        assert [r[0] for r in response["result"]] == [case[2] for case in cases]
    response, _ = read_message(writer)
    assert response["ok"], response
    assert response["result"]
    assert "ジャンヌ" in "".join(row[1] for row in response["result"])
