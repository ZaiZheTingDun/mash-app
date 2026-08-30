"""Lifecycle and IPC proxy for the disposable mash-cv OCR worker."""

from __future__ import annotations

import os
import queue
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from typing import Optional

import numpy as np

from mash_cv.ocr_protocol import read_message, write_message


OCR_WORKER_MAX_REQUESTS = 200
OCR_WORKER_MAX_DETECT_REQUESTS = 25
OCR_WORKER_MAX_LIFETIME_SECONDS = 30 * 60
# Two attempts, including process startup and forced cleanup, must fit inside
# the Rust client's 60-second recoverable OCR command timeout.
OCR_WORKER_PING_TIMEOUT_SECONDS = 5.0
OCR_WORKER_REQUEST_TIMEOUT_SECONDS = 15.0


@dataclass
class _PreparedImages:
    descriptors: list[dict]
    arrays: list[np.ndarray]

    @property
    def payload_parts(self) -> list[memoryview]:
        return [memoryview(array).cast("B") for array in self.arrays]


def _prepare_images(images: list[np.ndarray]) -> _PreparedImages:
    descriptors: list[dict] = []
    arrays: list[np.ndarray] = []
    offset = 0
    for image in images:
        array = np.ascontiguousarray(image)
        if array.dtype != np.uint8 or array.ndim not in (2, 3):
            raise ValueError(
                f"OCR image must be a 2D/3D uint8 array, got shape={array.shape} dtype={array.dtype}"
            )
        length = int(array.nbytes)
        descriptors.append(
            {
                "offset": offset,
                "length": length,
                "shape": list(array.shape),
                "dtype": array.dtype.str,
            }
        )
        arrays.append(array)
        offset += length
    return _PreparedImages(descriptors, arrays)


class _WorkerProcess:
    def __init__(self, generation: int) -> None:
        env = os.environ.copy()
        env["MASH_CV_PROCESS_MODE"] = "ocr-worker"
        env["PYTHONUNBUFFERED"] = "1"
        command = (
            [sys.executable]
            if getattr(sys, "frozen", False)
            else [sys.executable, "-m", "mash_cv"]
        )
        self.process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            bufsize=0,
        )
        self.generation = generation
        self.responses: queue.Queue[tuple[dict, bytes] | BaseException] = queue.Queue()
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()

    def _read_stdout(self) -> None:
        assert self.process.stdout is not None
        try:
            while True:
                self.responses.put(read_message(self.process.stdout))
        except BaseException as exc:  # EOF wakes a request waiting on the worker
            self.responses.put(exc)

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        for raw in iter(self.process.stderr.readline, b""):
            line = raw.decode("utf-8", errors="replace").strip()
            if line:
                print(f"[mash-cv-ocr stderr] {line}", file=sys.stderr, flush=True)

    def request(
        self,
        header: dict,
        payload_parts: list[memoryview],
        timeout: float,
    ) -> dict:
        if self.process.poll() is not None:
            raise RuntimeError(f"OCR worker exited with code {self.process.returncode}")
        assert self.process.stdin is not None
        write_message(self.process.stdin, header, payload_parts)
        response = self.responses.get(timeout=timeout)
        if isinstance(response, BaseException):
            raise RuntimeError(f"OCR worker response failed: {response}") from response
        response_header, response_payload = response
        if response_payload:
            raise RuntimeError("OCR worker returned an unexpected binary payload")
        if response_header.get("id") != header.get("id"):
            raise RuntimeError(
                f"OCR worker response id mismatch: expected {header.get('id')}, "
                f"got {response_header.get('id')}"
            )
        return response_header

    def close(self) -> None:
        if self.process.poll() is None:
            try:
                self.request({"id": 0, "action": "quit"}, [], 1.0)
            except Exception:  # noqa: BLE001 - termination below is authoritative
                pass
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=1.0)
        for pipe in (self.process.stdin, self.process.stdout, self.process.stderr):
            if pipe is not None:
                pipe.close()


class OcrWorkerClient:
    """RapidOCR-compatible facade backed by a disposable child process."""

    def __init__(self, server: str) -> None:
        self.server = server if server in ("JP", "CN") else "JP"
        self._worker: Optional[_WorkerProcess] = None
        self._next_id = 1
        self._generation = 0
        self._request_count = 0
        self._detect_request_count = 0
        self._started_at = 0.0

    def _spawn(self) -> _WorkerProcess:
        self._generation += 1
        worker = _WorkerProcess(self._generation)
        try:
            response = worker.request(
                {"id": 0, "action": "ping"},
                [],
                OCR_WORKER_PING_TIMEOUT_SECONDS,
            )
            if not response.get("ok"):
                raise RuntimeError(str(response.get("error", "OCR worker ping failed")))
        except Exception:
            worker.close()
            raise
        self._worker = worker
        self._request_count = 0
        self._detect_request_count = 0
        self._started_at = time.monotonic()
        print(
            f"[mash-cv-ocr] spawned pid={worker.process.pid} generation={self._generation} "
            f"server={self.server}",
            file=sys.stderr,
            flush=True,
        )
        return worker

    def _ensure_worker(self) -> _WorkerProcess:
        if self._worker is not None and (
            self._request_count >= OCR_WORKER_MAX_REQUESTS
            or self._detect_request_count >= OCR_WORKER_MAX_DETECT_REQUESTS
            or time.monotonic() - self._started_at >= OCR_WORKER_MAX_LIFETIME_SECONDS
        ):
            self.close("limit")
        return self._worker or self._spawn()

    def _request(
        self,
        action: str,
        images: list[np.ndarray],
        *,
        recycle_after: bool = False,
    ) -> dict:
        prepared = _prepare_images(images)
        last_error: Optional[BaseException] = None
        for attempt in range(2):
            worker = self._ensure_worker()
            request_id = self._next_id
            self._next_id += 1
            header = {
                "id": request_id,
                "action": action,
                "server": self.server,
                "images": prepared.descriptors,
            }
            try:
                response = worker.request(
                    header,
                    prepared.payload_parts,
                    OCR_WORKER_REQUEST_TIMEOUT_SECONDS,
                )
                if not response.get("ok"):
                    raise RuntimeError(str(response.get("error", "OCR worker request failed")))
                self._request_count += 1
                if action == "detect":
                    self._detect_request_count += 1
                if recycle_after:
                    self.close("large-detect")
                elif self._request_count >= OCR_WORKER_MAX_REQUESTS:
                    self.close("request-limit")
                elif self._detect_request_count >= OCR_WORKER_MAX_DETECT_REQUESTS:
                    self.close("detect-limit")
                elif time.monotonic() - self._started_at >= OCR_WORKER_MAX_LIFETIME_SECONDS:
                    self.close("lifetime-limit")
                return response
            except Exception as exc:  # noqa: BLE001 - retry once with a fresh worker
                last_error = exc
                self.close(f"request-failed-attempt-{attempt + 1}")
        raise RuntimeError(f"OCR worker failed after retry: {last_error}") from last_error

    def __call__(self, image: np.ndarray):
        return self.detect(image)

    def detect(self, image: np.ndarray, *, recycle_after: bool = False):
        response = self._request("detect", [image], recycle_after=recycle_after)
        result = [tuple(item) for item in response.get("result", [])]
        return result, float(response.get("elapsed", 0.0))

    def text_recognizer(self, images: list[np.ndarray]):
        response = self._request("recognize_batch", images)
        result = [tuple(item) for item in response.get("result", [])]
        return result, float(response.get("elapsed", 0.0))

    def close(self, reason: str = "requested") -> None:
        worker = self._worker
        self._worker = None
        if worker is None:
            return
        lifetime = max(0.0, time.monotonic() - self._started_at)
        print(
            f"[mash-cv-ocr] recycling pid={worker.process.pid} generation={worker.generation} "
            f"reason={reason} requests={self._request_count} "
            f"detects={self._detect_request_count} lifetime={lifetime:.1f}s",
            file=sys.stderr,
            flush=True,
        )
        worker.close()
        self._request_count = 0
        self._detect_request_count = 0
        self._started_at = 0.0
