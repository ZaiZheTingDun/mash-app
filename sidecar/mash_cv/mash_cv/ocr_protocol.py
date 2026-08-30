"""Binary framing shared by the mash-cv process and its OCR worker."""

from __future__ import annotations

import json
import struct
from typing import BinaryIO, Iterable


_PREFIX = struct.Struct(">IQ")
_MAX_HEADER_BYTES = 1024 * 1024
_MAX_PAYLOAD_BYTES = 256 * 1024 * 1024


class OcrProtocolError(RuntimeError):
    """Raised when an OCR worker frame is truncated or malformed."""


def _read_exact(stream: BinaryIO, size: int) -> bytes:
    chunks: list[bytes] = []
    remaining = size
    while remaining:
        chunk = stream.read(remaining)
        if not chunk:
            raise EOFError("OCR worker pipe closed")
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def read_message(stream: BinaryIO) -> tuple[dict, bytes]:
    prefix = _read_exact(stream, _PREFIX.size)
    header_size, payload_size = _PREFIX.unpack(prefix)
    if header_size > _MAX_HEADER_BYTES:
        raise OcrProtocolError(f"OCR header too large: {header_size}")
    if payload_size > _MAX_PAYLOAD_BYTES:
        raise OcrProtocolError(f"OCR payload too large: {payload_size}")

    try:
        header = json.loads(_read_exact(stream, header_size))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise OcrProtocolError(f"invalid OCR header: {exc}") from exc
    if not isinstance(header, dict):
        raise OcrProtocolError("OCR header must be a JSON object")
    return header, _read_exact(stream, payload_size)


def write_message(
    stream: BinaryIO,
    header: dict,
    payload_parts: Iterable[bytes | bytearray | memoryview] = (),
) -> None:
    header_bytes = json.dumps(header, ensure_ascii=False, separators=(",", ":")).encode(
        "utf-8"
    )
    parts = [memoryview(part).cast("B") for part in payload_parts]
    payload_size = sum(len(part) for part in parts)
    if len(header_bytes) > _MAX_HEADER_BYTES:
        raise OcrProtocolError(f"OCR header too large: {len(header_bytes)}")
    if payload_size > _MAX_PAYLOAD_BYTES:
        raise OcrProtocolError(f"OCR payload too large: {payload_size}")

    stream.write(_PREFIX.pack(len(header_bytes), payload_size))
    stream.write(header_bytes)
    for part in parts:
        stream.write(part)
    stream.flush()
