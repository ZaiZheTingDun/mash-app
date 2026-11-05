"""Disposable RapidOCR/ONNX worker for the long-running mash-cv process."""

from __future__ import annotations

import os
import sys
import time
from typing import BinaryIO, Callable, Optional

import numpy as np

from mash_cv.ocr_protocol import OcrProtocolError, read_message, write_message


def _models_dir() -> Optional[str]:
    candidates: list[str] = []
    configured = os.environ.get("MASH_CV_MODELS_DIR")
    if configured:
        candidates.append(configured)
    candidates.append(os.path.join(os.path.dirname(__file__), "..", "..", "cv-runtime", "models"))
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        candidates.append(os.path.join(meipass, "mash_cv", "models"))
    return next((path for path in candidates if os.path.isdir(path)), None)


class RapidOcrEngine:
    """Own all RapidOCR sessions inside the disposable worker process."""

    def __init__(self, server: str) -> None:
        try:
            from rapidocr import RapidOCR
            from rapidocr.utils.typings import EngineType
        except ImportError as exc:
            raise RuntimeError(
                "PP-OCRv6 small requires the updated native CV runtime; "
                "install the runtime built with RapidOCR 3.10.0"
            ) from exc

        models_dir = _models_dir()
        filenames = {
            "Det.model_path": "PP-OCRv6_det_small.onnx",
            "Rec.model_path": "PP-OCRv6_rec_small.onnx",
            "Cls.model_path": "ch_ppocr_mobile_v2.0_cls_mobile.onnx",
            "Rec.rec_keys_path": "PP-OCRv6_small_rec_dict.txt",
        }
        paths = {key: os.path.join(models_dir or "", name) for key, name in filenames.items()}
        missing = [path for path in paths.values() if not os.path.isfile(path)]
        if models_dir is None or missing:
            raise FileNotFoundError(
                "PP-OCRv6 small models are missing; install the matching CV runtime "
                f"(dir={models_dir!r}, missing={missing!r})"
            )
        print(
            f"[mash-cv-ocr] initializing unified PP-OCRv6 small "
            f"server={server} backend=onnxruntime threads=1 models={models_dir}",
            file=sys.stderr, flush=True,
        )
        self._engine = RapidOCR(params={
            **paths,
            "Global.log_level": "warning",
            "Global.max_side_len": 2000,
            "Global.min_side_len": 30,
            "Det.engine_type": EngineType.ONNXRUNTIME,
            "Rec.engine_type": EngineType.ONNXRUNTIME,
            "Cls.engine_type": EngineType.ONNXRUNTIME,
            "EngineConfig.onnxruntime.intra_op_num_threads": 1,
            "EngineConfig.onnxruntime.inter_op_num_threads": 1,
        })
        # RapidOCR 3.10 loads sessions lazily. Keep the version-pinned private
        # batch entry inside this adapter; worker IPC remains unchanged.
        self._recognizer = self._engine._load_rec_model()
        print("[mash-cv-ocr] PP-OCRv6 small recognizer ready", file=sys.stderr, flush=True)

    def detect(self, image: np.ndarray) -> list[list]:
        result = self._engine(image)
        if result.boxes is None or result.txts is None:
            return []
        return [
            [np.asarray(box, dtype=np.float32).tolist(), str(text),
             float(confidence) if confidence is not None else 0.0]
            for box, text, confidence in zip(result.boxes, result.txts, result.scores)
        ]

    def recognize_batch(self, images: list[np.ndarray]) -> list[list]:
        if not images:
            return []
        from rapidocr.ch_ppocr_rec.typings import TextRecInput

        result = self._recognizer(TextRecInput(img=images))
        if result.txts is None:
            return []
        return [
            [str(text), float(confidence) if confidence is not None else 0.0]
            for text, confidence in zip(result.txts, result.scores)
        ]


def _decode_images(header: dict, payload: bytes) -> list[np.ndarray]:
    images: list[np.ndarray] = []
    for descriptor in header.get("images", []):
        try:
            offset = int(descriptor["offset"])
            length = int(descriptor["length"])
            shape = tuple(int(value) for value in descriptor["shape"])
            dtype = np.dtype(str(descriptor["dtype"]))
        except (KeyError, TypeError, ValueError) as exc:
            raise OcrProtocolError(f"invalid OCR image descriptor: {descriptor!r}") from exc
        if offset < 0 or length < 0 or offset + length > len(payload):
            raise OcrProtocolError("OCR image descriptor exceeds payload")
        expected = int(np.prod(shape, dtype=np.int64)) * dtype.itemsize
        if expected != length:
            raise OcrProtocolError(
                f"OCR image length mismatch: expected {expected}, got {length}"
            )
        image = np.frombuffer(payload, dtype=dtype, count=expected // dtype.itemsize, offset=offset)
        images.append(image.reshape(shape))
    return images


def serve(
    reader: BinaryIO,
    writer: BinaryIO,
    engine_factory: Callable[[str], RapidOcrEngine] = RapidOcrEngine,
) -> None:
    engine = None
    engine_server: Optional[str] = None
    while True:
        try:
            request, payload = read_message(reader)
        except EOFError:
            return
        request_id = request.get("id")
        action = request.get("action")
        if action == "quit":
            write_message(writer, {"id": request_id, "ok": True})
            return
        if action == "ping":
            write_message(writer, {"id": request_id, "ok": True})
            continue

        try:
            server = str(request.get("server", "JP")).upper()
            if engine is None or engine_server != server:
                engine = engine_factory(server)
                engine_server = server
            images = _decode_images(request, payload)
            started = time.monotonic()
            if action == "detect":
                if len(images) != 1:
                    raise OcrProtocolError("detect expects exactly one image")
                result = engine.detect(images[0])
            elif action == "recognize_batch":
                result = engine.recognize_batch(images)
            else:
                raise OcrProtocolError(f"unknown OCR worker action: {action}")
            write_message(
                writer,
                {
                    "id": request_id,
                    "ok": True,
                    "result": result,
                    "elapsed": time.monotonic() - started,
                },
            )
        except Exception as exc:  # noqa: BLE001 - isolate worker failures
            write_message(
                writer,
                {"id": request_id, "ok": False, "error": f"{type(exc).__name__}: {exc}"},
            )


def main() -> None:
    serve(sys.stdin.buffer, sys.stdout.buffer)
