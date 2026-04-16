"""
scrcpy realtime video stream for mash-cv.

Pushes a pinned `scrcpy-server.jar` (v2.7) to the device, starts it over adb
using a forwarded TCP tunnel, decodes the H.264 stream with PyAV and keeps the
latest BGR frame in memory under a lock. The rest of the sidecar reads frames
via :meth:`ScrcpyStream.get_latest_bgr`, so the file-based CV API no longer
needs to hit disk for every poll.

Protocol reference: https://github.com/Genymobile/scrcpy/blob/v2.7/server
"""

from __future__ import annotations

import os
import random
import socket
import struct
import subprocess
import sys
import threading
import time
from typing import Optional

import cv2
import numpy as np

SCRCPY_VERSION = "2.7"

DEVICE_NAME_FIELD_LENGTH = 64
CODEC_META_LENGTH = 12
FRAME_META_LENGTH = 12

CONFIG_FLAG = 1 << 63
KEY_FRAME_FLAG = 1 << 62
PTS_MASK = (1 << 62) - 1


def _adb_base(serial: Optional[str]) -> list[str]:
    return ["adb", "-s", serial] if serial else ["adb"]


def _clean_env() -> dict[str, str]:
    """Return an env without PyInstaller's bundle-specific dyld hints.

    When the sidecar is frozen with PyInstaller, ``DYLD_LIBRARY_PATH``/
    ``LD_LIBRARY_PATH`` are rewritten to point inside the onefile unpack
    directory. If those leak into child processes they break ``adb`` (the
    host client) and any other system binary we shell out to, often in
    non-obvious ways (e.g. the adb tunnel half-works and then closes).
    PyInstaller stashes the originals under ``*_ORIG``; restore them.
    """
    env = os.environ.copy()
    for key in ("DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "LD_LIBRARY_PATH"):
        orig = env.pop(f"{key}_ORIG", None)
        if orig is not None:
            env[key] = orig
        else:
            env.pop(key, None)
    return env


def _run_adb(args: list[str], check: bool = True) -> subprocess.CompletedProcess:
    proc = subprocess.run(args, capture_output=True, text=True, env=_clean_env())
    if check and proc.returncode != 0:
        raise RuntimeError(
            f"adb {' '.join(args[1:])} failed ({proc.returncode}): {proc.stderr.strip()}"
        )
    return proc


def _alloc_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class ScrcpyStream:
    """Manages a single device-side scrcpy server and its decoded frame cache."""

    def __init__(
        self,
        jar_path: str,
        serial: Optional[str] = None,
        max_size: int = 0,
        bit_rate: int = 8_000_000,
    ) -> None:
        self.jar_path = jar_path
        self.serial = serial
        self.max_size = max_size
        self.bit_rate = bit_rate

        self.scid = random.randint(0, 0x7FFFFFFF)
        self.socket_name = f"scrcpy_{self.scid:08x}"
        self.port: Optional[int] = None

        self._sock: Optional[socket.socket] = None
        self._server_proc: Optional[subprocess.Popen] = None
        self._stderr_thread: Optional[threading.Thread] = None
        self._decode_thread: Optional[threading.Thread] = None
        self._stop_event = threading.Event()

        self._latest_frame: Optional[np.ndarray] = None
        self._latest_lock = threading.Lock()

        self.width = 0
        self.height = 0
        self.device_name = ""

    # -- lifecycle -----------------------------------------------------------

    def start(self) -> tuple[int, int]:
        """Push the jar, start the server, handshake, and begin decoding.

        Returns the (width, height) reported by the device.
        """
        self._push_jar()
        self.port = _alloc_free_port()
        self._adb_forward_add()

        try:
            self._spawn_server()
            self._sock = self._connect_and_handshake_dummy()
            self._read_device_meta()
        except Exception:
            self.stop()
            raise

        self._stop_event.clear()
        self._decode_thread = threading.Thread(
            target=self._decode_loop,
            name="scrcpy-decode",
            daemon=True,
        )
        self._decode_thread.start()

        return self.width, self.height

    def stop(self) -> None:
        self._stop_event.set()

        if self._sock is not None:
            try:
                self._sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            try:
                self._sock.close()
            except OSError:
                pass
            self._sock = None

        if self._decode_thread is not None and self._decode_thread.is_alive():
            self._decode_thread.join(timeout=1.0)
        self._decode_thread = None

        if self._server_proc is not None:
            try:
                self._server_proc.terminate()
                try:
                    self._server_proc.wait(timeout=1.5)
                except subprocess.TimeoutExpired:
                    self._server_proc.kill()
            except OSError:
                pass
            self._server_proc = None

        if self.port is not None:
            _run_adb(
                _adb_base(self.serial) + ["forward", "--remove", f"tcp:{self.port}"],
                check=False,
            )
            self.port = None

    # -- frame access --------------------------------------------------------

    def get_latest_bgr(self) -> Optional[np.ndarray]:
        with self._latest_lock:
            if self._latest_frame is None:
                return None
            return self._latest_frame.copy()

    def wait_for_frame(self, timeout: float = 5.0) -> Optional[np.ndarray]:
        """Block until at least one frame has been decoded or ``timeout`` s pass."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            frame = self.get_latest_bgr()
            if frame is not None:
                return frame
            if (
                self._decode_thread is None
                or not self._decode_thread.is_alive()
            ):
                return None
            time.sleep(0.05)
        return self.get_latest_bgr()

    def get_latest_jpeg(
        self, quality: int = 85, wait: float = 0.0
    ) -> Optional[bytes]:
        if wait > 0:
            frame = self.wait_for_frame(timeout=wait)
        else:
            frame = self.get_latest_bgr()
        if frame is None:
            return None
        ok, buf = cv2.imencode(".jpg", frame, [int(cv2.IMWRITE_JPEG_QUALITY), quality])
        return buf.tobytes() if ok else None

    # -- internal ------------------------------------------------------------

    def _push_jar(self) -> None:
        _run_adb(
            _adb_base(self.serial)
            + ["push", self.jar_path, "/data/local/tmp/scrcpy-server.jar"]
        )

    def _adb_forward_add(self) -> None:
        _run_adb(
            _adb_base(self.serial)
            + ["forward", f"tcp:{self.port}", f"localabstract:{self.socket_name}"]
        )

    def _spawn_server(self) -> None:
        env = _clean_env()
        cmd = _adb_base(self.serial) + [
            "shell",
            "CLASSPATH=/data/local/tmp/scrcpy-server.jar",
            "app_process",
            "/",
            "com.genymobile.scrcpy.Server",
            SCRCPY_VERSION,
            f"scid={self.scid:08x}",
            "log_level=warn",
            "audio=false",
            "control=false",
            "clipboard_autosync=false",
            "cleanup=true",
            "tunnel_forward=true",
            "video_codec=h264",
            f"video_bit_rate={self.bit_rate}",
            f"max_size={self.max_size}",
        ]
        # ``adb shell`` folds the device-side stderr into stdout, so merge our
        # local handles and read from stdout to capture every server message.
        # Crucially, ``stdin=DEVNULL``: otherwise the subprocess inherits the
        # sidecar's own stdin (which is a pipe of JSON commands from Rust) and
        # forwards those bytes to the device-side scrcpy server, which causes
        # it to close the video tunnel as soon as spurious input arrives.
        # ``stdin=DEVNULL`` so the ``adb shell`` subprocess does not inherit
        # (and consume from) the sidecar's real stdin, which carries our JSON
        # command pipe from Rust.
        self._server_proc = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=env,
        )

        def pump_log(pipe) -> None:
            for raw in iter(pipe.readline, b""):
                line = raw.decode("utf-8", errors="replace").rstrip()
                if line:
                    print(f"[scrcpy-server] {line}", file=sys.stderr, flush=True)

        self._stderr_thread = threading.Thread(
            target=pump_log,
            args=(self._server_proc.stdout,),
            name="scrcpy-log",
            daemon=True,
        )
        self._stderr_thread.start()

    def _connect_and_handshake_dummy(
        self, attempts: int = 150, delay: float = 0.1
    ) -> socket.socket:
        """Connect to the forwarded port and consume the 1-byte dummy handshake.

        ``adb forward`` accepts the local TCP connection before the device-side
        abstract socket is actually bound, so the first few connect attempts
        succeed at the kernel level but the device then closes the tunnel
        immediately (``recv`` returns 0). We treat that as "not ready yet" and
        retry until the server writes its dummy byte, which signals that the
        tunnel is live and we are talking to scrcpy.
        """
        last_err: Optional[Exception] = None
        for attempt in range(attempts):
            if self._server_proc is not None and self._server_proc.poll() is not None:
                raise RuntimeError(
                    f"scrcpy server exited before accepting (code {self._server_proc.returncode})"
                )
            try:
                sock = socket.create_connection(("127.0.0.1", self.port), timeout=2.0)
            except OSError as exc:
                last_err = exc
                time.sleep(delay)
                continue

            try:
                sock.settimeout(2.0)
                chunk = sock.recv(1)
                if not chunk:
                    last_err = ConnectionError("tunnel closed before dummy byte")
                    sock.close()
                    time.sleep(delay)
                    continue
                if chunk != b"\x00":
                    sock.close()
                    raise RuntimeError(
                        f"unexpected scrcpy dummy byte: {chunk!r}"
                    )
                sock.settimeout(None)
                return sock
            except socket.timeout as exc:
                last_err = exc
                try:
                    sock.close()
                except OSError:
                    pass
                time.sleep(delay)
                continue
            except OSError as exc:
                last_err = exc
                try:
                    sock.close()
                except OSError:
                    pass
                time.sleep(delay)
                continue
        raise RuntimeError(
            f"failed to reach scrcpy server on port {self.port}: {last_err}"
        )

    def _recv_exact(self, n: int) -> bytes:
        assert self._sock is not None
        buf = bytearray()
        while len(buf) < n:
            chunk = self._sock.recv(n - len(buf))
            if not chunk:
                raise ConnectionError("scrcpy socket closed")
            buf.extend(chunk)
        return bytes(buf)

    def _read_device_meta(self) -> None:
        # send_device_meta (default true) -- 64 bytes device name
        raw_name = self._recv_exact(DEVICE_NAME_FIELD_LENGTH)
        self.device_name = raw_name.split(b"\x00", 1)[0].decode("utf-8", errors="replace")
        # send_codec_meta (default true) -- codec id + width + height
        codec_meta = self._recv_exact(CODEC_META_LENGTH)
        _codec_id, width, height = struct.unpack(">III", codec_meta)
        self.width = width
        self.height = height
        print(
            f"[scrcpy] connected to {self.device_name!r} @ {width}x{height}",
            file=sys.stderr,
            flush=True,
        )

    def _decode_loop(self) -> None:
        import av
        codec = av.CodecContext.create("h264", "r")
        first_frame_logged = False
        try:
            while not self._stop_event.is_set():
                try:
                    header = self._recv_exact(FRAME_META_LENGTH)
                except (ConnectionError, OSError):
                    break

                pts_flags, size = struct.unpack(">QI", header)
                if size == 0:
                    continue
                try:
                    payload = self._recv_exact(size)
                except (ConnectionError, OSError):
                    break

                is_config = bool(pts_flags & CONFIG_FLAG)

                try:
                    packet = av.Packet(payload)
                    for frame in codec.decode(packet):
                        bgr = frame.to_ndarray(format="bgr24")
                        with self._latest_lock:
                            self._latest_frame = bgr
                        if not first_frame_logged:
                            first_frame_logged = True
                            print(
                                f"[scrcpy] first frame decoded ({bgr.shape[1]}x{bgr.shape[0]})",
                                file=sys.stderr,
                                flush=True,
                            )
                except av.InvalidDataError as exc:
                    if not is_config:
                        print(
                            f"[scrcpy] decode error: {exc}",
                            file=sys.stderr,
                            flush=True,
                        )
                except Exception as exc:  # noqa: BLE001
                    print(
                        f"[scrcpy] unexpected decode error: {exc}",
                        file=sys.stderr,
                        flush=True,
                    )
        finally:
            try:
                for frame in codec.decode(None):
                    bgr = frame.to_ndarray(format="bgr24")
                    with self._latest_lock:
                        self._latest_frame = bgr
            except Exception:  # noqa: BLE001
                pass
