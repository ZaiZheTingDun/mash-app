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
    candidates.append(os.path.join(os.path.dirname(__file__), "models"))
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        candidates.append(os.path.join(meipass, "mash_cv", "models"))
    return next((path for path in candidates if os.path.isdir(path)), None)


class RapidOcrEngine:
    """Own all RapidOCR sessions inside the disposable worker process."""

    def __init__(self, server: str) -> None:
        from rapidocr_onnxruntime import RapidOCR

        resolved = server if server in ("JP", "CN") else "JP"
        models_dir = _models_dir()
        if resolved == "CN":
            rec_filename = "chinese_PP-OCRv4_rec_infer.onnx"
            keys_filename = "ppocr_keys_v1.txt"
        else:
            rec_filename = "japan_PP-OCRv4_rec_infer.onnx"
            keys_filename = "japan_dict.txt"

        rec_model = os.path.join(models_dir, rec_filename) if models_dir else None
        rec_keys = os.path.join(models_dir, keys_filename) if models_dir else None
        if rec_model and rec_keys and os.path.isfile(rec_model) and os.path.isfile(rec_keys):
            print(
                f"[mash-cv-ocr] using {resolved} rec model: {rec_model}",
                file=sys.stderr,
                flush=True,
            )
            self._engine = RapidOCR(rec_model_path=rec_model, rec_keys_path=rec_keys)
        else:
            print(
                f"[mash-cv-ocr] WARNING: {resolved} rec model not found "
                f"(dir={models_dir!r} rec={rec_model!r} keys={rec_keys!r}); "
                "falling back to the default RapidOCR model",
                file=sys.stderr,
                flush=True,
            )
            self._engine = RapidOCR()

    def detect(self, image: np.ndarray) -> list[list]:
        raw, _elapsed = self._engine(image)
        serialized: list[list] = []
        for box, text, confidence in raw or []:
            serialized.append(
                [
                    np.asarray(box, dtype=np.float32).tolist(),
                    str(text),
                    float(confidence) if confidence is not None else 0.0,
                ]
            )
        return serialized

    def recognize_batch(self, images: list[np.ndarray]) -> list[list]:
        results, _elapsed = self._engine.text_recognizer(images)
        return [
            [str(text), float(confidence) if confidence is not None else 0.0]
            for text, confidence in (results or [])
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
