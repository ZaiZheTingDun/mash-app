"""MuMu Player frame source: renderer IPC, with a window-capture fallback.

Why this exists
---------------
Scrcpy (and ``adb screencap``) read pixels through Android's SurfaceFlinger.
When the game marks its window with ``FLAG_SECURE``, every one of those paths
returns black pixels, so template matching has nothing to work with.

Two host-side ways out, tried in this order:

1. **Renderer IPC** (``external_renderer_ipc.dll`` -> ``nemu_capture_display``).
   Fast, native resolution, no window focus needed. *However* MuMu composites
   that display itself and still drops secure layers, so with FGO (or any
   FLAG_SECURE app) in the foreground the frame is entirely black. Measured on
   MuMu 12 / Android 15: 1920x1080, mean pixel 0.00, max 0.

2. **Window capture** (``PrintWindow`` on MuMu's ``nemudisplay`` child window,
   class ``nemuwin``). Asks the Qt host window to paint itself into a memory
   DC, so we read the pixels the user actually sees on the desktop.
   FLAG_SECURE never enters the picture because the flag lives inside the
   guest. Verified to return live, sharp 16:9 game frames while the IPC path
   is black.

``source`` in ``capture_config.json`` selects the strategy:

    "auto"    try IPC, fall back to window capture when the frame is blank
    "ipc"     IPC only (fails loudly if the frame is black)
    "window"  window capture only
    "scrcpy"  stock behaviour, this module is not used

Any failure still falls through to scrcpy in ``cv.py``, so leaving the config
out keeps stock behaviour.
"""

from __future__ import annotations

import ctypes
import glob
import json
import os
import sys
import threading
import time
from typing import Optional

import cv2
import numpy as np

# ---------------------------------------------------------------------------
# Win32 typedefs
#
# ``ctypes.wintypes`` is not always present: PyInstaller runtimes ship a
# trimmed ``ctypes`` package without it, and importing it there fails at
# module load time (which would silently disable this whole frame source).
# The handful of types needed here are one-liners, so define them locally.
# ---------------------------------------------------------------------------

_HWND = ctypes.c_void_p
_HDC = ctypes.c_void_p
_HBITMAP = ctypes.c_void_p
_HGDIOBJ = ctypes.c_void_p
_LPARAM = ctypes.c_ssize_t
_LPWSTR = ctypes.c_wchar_p
_BOOL = ctypes.c_int
_UINT = ctypes.c_uint
_DWORD = ctypes.c_ulong
_WORD = ctypes.c_ushort


class _RECT(ctypes.Structure):
    _fields_ = [
        ("left", ctypes.c_long),
        ("top", ctypes.c_long),
        ("right", ctypes.c_long),
        ("bottom", ctypes.c_long),
    ]


# Only used as a last resort: normally the install root is auto-detected by
# locating the renderer DLL, so a machine that never had MuMu in the default
# place still works. Empty means "detect only".
DEFAULT_ROOT = ""
DEFAULT_PACKAGE = "com.bilibili.fatego"

_DLL_SUBPATHS = (
    os.path.join("shell", "sdk", "external_renderer_ipc.dll"),
    os.path.join("nx_main", "sdk", "external_renderer_ipc.dll"),
    os.path.join("nx_device", "15.0", "shell", "sdk", "external_renderer_ipc.dll"),
    os.path.join("nx_device", "12.0", "shell", "sdk", "external_renderer_ipc.dll"),
)

# The install root moves whenever the emulator is reinstalled (``MuMu Player
# 12`` -> ``MuMu``), so hard-coding one location silently disables the whole
# backend after an upgrade. Look for the renderer DLL itself instead.
_ROOT_GLOBS = (
    os.path.join("Program Files", "Netease", "MuMu*"),
    os.path.join("Program Files (x86)", "Netease", "MuMu*"),
    os.path.join("Netease", "MuMu*"),
    "MuMu*",
)
_ROOT_DRIVES = "CDEFGH"

_CONFIG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "capture_config.json")


def _dll_candidates(root: str) -> list[str]:
    """Every plausible location of external_renderer_ipc.dll under *root*."""
    if not root or not os.path.isdir(root):
        return []
    found: list[str] = []
    for sub in _DLL_SUBPATHS:
        path = os.path.join(root, sub)
        if os.path.isfile(path):
            found.append(path)
    # MuMu rearranges these folders between releases; fall back to a shallow
    # search so a layout change does not silently disable the whole backend.
    for pattern in (
        os.path.join(root, "*", "sdk", "external_renderer_ipc.dll"),
        os.path.join(root, "*", "*", "sdk", "external_renderer_ipc.dll"),
        os.path.join(root, "*", "*", "shell", "sdk", "external_renderer_ipc.dll"),
    ):
        for path in glob.glob(pattern):
            if os.path.isfile(path) and path not in found:
                found.append(path)
    return found


def _scan_installed_roots() -> list[str]:
    """Find MuMu installs on the local drives by locating the renderer DLL."""
    hits: list[str] = []
    for drive in _ROOT_DRIVES:
        for rel in _ROOT_GLOBS:
            for root in glob.glob(f"{drive}:\\{rel}"):
                if root in hits or not os.path.isdir(root):
                    continue
                if _dll_candidates(root):
                    hits.append(root)
    return hits


def resolve_root(preferred: Optional[str] = None) -> str:
    """Pick MuMu's install root: explicit setting first, then auto-detect."""
    for candidate in (preferred, DEFAULT_ROOT):
        if candidate and _dll_candidates(candidate):
            return candidate
    scanned = _scan_installed_roots()
    if scanned:
        return scanned[0]
    return preferred or DEFAULT_ROOT

# PrintWindow flag: ask the window to render its full content, including the
# GPU-composited layer that a plain BitBlt would miss.
_PW_RENDERFULLCONTENT = 0x00000002

_SURFACE_CLASSES = ("nemuwin",)
_SURFACE_TITLES = ("nemudisplay",)

# Smallest client rect we accept as "a real guest display". MuMu's
# multi-instance manager hosts a ~192x108 preview pane that otherwise passes
# every other check and quietly produces a stream of nothing.
MIN_SURFACE_W = 320
MIN_SURFACE_H = 180

_stdout_isolated = False


# ---------------------------------------------------------------------------
# config
# ---------------------------------------------------------------------------


def load_config() -> dict:
    """Read capture_config.json, then let env vars override it."""
    cfg: dict = {}
    try:
        with open(_CONFIG_PATH, "r", encoding="utf-8") as fh:
            loaded = json.load(fh)
        if isinstance(loaded, dict):
            cfg.update(loaded)
    except FileNotFoundError:
        pass
    except Exception as exc:  # noqa: BLE001
        print(f"[mumu-capture] bad capture_config.json: {exc}", file=sys.stderr)

    env_source = os.environ.get("MASH_CV_CAPTURE")
    if env_source:
        cfg["source"] = env_source.strip().lower()
    env_root = os.environ.get("MASH_CV_MUMU_ROOT")
    if env_root:
        cfg["mumuRoot"] = env_root
    env_index = os.environ.get("MASH_CV_MUMU_VMINDEX")
    if env_index:
        try:
            cfg["vmIndex"] = int(env_index)
        except ValueError:
            pass
    env_window = os.environ.get("MASH_CV_MUMU_WINDOW")
    if env_window:
        cfg["windowTitle"] = env_window
    cfg.setdefault("source", "scrcpy")
    return cfg


def _isolate_native_stdout() -> None:
    """Keep the DLL's C-level printf off the sidecar's JSON-RPC stdout.

    ``nemu_connect`` writes diagnostics ("product: ...", "nemu_connect
    instance_name: ...") straight to fd 1. The Rust host parses every stdout
    line as JSON, so those lines would poison the protocol. We hand Python a
    duplicate of the real fd and point fd 1 at the null device, which is what
    the DLL ends up writing to.
    """
    global _stdout_isolated
    if _stdout_isolated:
        return
    _stdout_isolated = True
    try:
        sys.stdout.flush()
        real_fd = os.dup(1)
        null_fd = os.open(os.devnull, os.O_WRONLY)
        os.dup2(null_fd, 1)
        os.close(null_fd)
        sys.stdout = os.fdopen(real_fd, "w", encoding="utf-8", newline="\n", buffering=1)
    except Exception as exc:  # noqa: BLE001
        print(f"[mumu-capture] stdout isolation skipped: {exc}", file=sys.stderr)


# ---------------------------------------------------------------------------
# DLL binding
# ---------------------------------------------------------------------------


class _NemuApi:
    def __init__(self, root: str) -> None:
        self.root = root
        self.lib = None
        self.path = None
        self._load()

    def _load(self) -> None:
        errors = []
        for candidate in _dll_candidates(self.root):
            if not os.path.exists(candidate):
                continue
            try:
                os.add_dll_directory(os.path.dirname(candidate))
            except Exception:  # noqa: BLE001
                pass
            try:
                self.lib = ctypes.WinDLL(candidate)
                self.path = candidate
                break
            except OSError as exc:
                errors.append(f"{candidate}: {exc}")
        if self.lib is None:
            detail = "; ".join(errors) if errors else "no candidate path exists"
            raise RuntimeError(f"cannot load external_renderer_ipc.dll under {self.root} ({detail})")

        c_int = ctypes.c_int
        lib = self.lib
        self.connect = lib.nemu_connect
        self.connect.argtypes = [ctypes.c_wchar_p, c_int]
        self.connect.restype = c_int
        self.disconnect = lib.nemu_disconnect
        self.disconnect.argtypes = [c_int]
        self.disconnect.restype = c_int
        self.get_display_id = lib.nemu_get_display_id
        self.get_display_id.argtypes = [c_int, ctypes.c_char_p, c_int]
        self.get_display_id.restype = c_int
        self.capture_display = lib.nemu_capture_display
        self.capture_display.argtypes = [
            c_int,
            c_int,
            c_int,
            ctypes.POINTER(c_int),
            ctypes.POINTER(c_int),
            ctypes.c_void_p,
        ]
        self.capture_display.restype = c_int


# ---------------------------------------------------------------------------
# Win32 window capture
# ---------------------------------------------------------------------------


class _BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", _DWORD),
        ("biWidth", ctypes.c_long),
        ("biHeight", ctypes.c_long),
        ("biPlanes", _WORD),
        ("biBitCount", _WORD),
        ("biCompression", _DWORD),
        ("biSizeImage", _DWORD),
        ("biXPelsPerMeter", ctypes.c_long),
        ("biYPelsPerMeter", ctypes.c_long),
        ("biClrUsed", _DWORD),
        ("biClrImportant", _DWORD),
    ]


class _WinApi:
    """Thin user32/gdi32 wrapper: find MuMu's render surface, paint it to a DC."""

    def __init__(self) -> None:
        self.user32 = ctypes.WinDLL("user32", use_last_error=True)
        self.gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
        u, g = self.user32, self.gdi32

        self._enum_proc = ctypes.WINFUNCTYPE(_BOOL, _HWND, _LPARAM)
        u.EnumWindows.argtypes = [self._enum_proc, _LPARAM]
        u.EnumChildWindows.argtypes = [_HWND, self._enum_proc, _LPARAM]
        u.GetWindowTextW.argtypes = [_HWND, _LPWSTR, ctypes.c_int]
        u.GetWindowTextLengthW.argtypes = [_HWND]
        u.GetClassNameW.argtypes = [_HWND, _LPWSTR, ctypes.c_int]
        u.IsWindow.argtypes = [_HWND]
        u.IsWindowVisible.argtypes = [_HWND]
        u.GetClientRect.argtypes = [_HWND, ctypes.POINTER(_RECT)]
        u.GetWindowRect.argtypes = [_HWND, ctypes.POINTER(_RECT)]
        u.MapWindowPoints.argtypes = [
            _HWND, _HWND, ctypes.POINTER(_RECT), _UINT
        ]
        u.MapWindowPoints.restype = ctypes.c_int
        u.IsIconic.argtypes = [_HWND]
        u.GetDC.argtypes = [_HWND]
        u.GetDC.restype = _HDC
        u.ReleaseDC.argtypes = [_HWND, _HDC]
        u.PrintWindow.argtypes = [_HWND, _HDC, _UINT]
        u.PrintWindow.restype = _BOOL

        g.CreateCompatibleDC.argtypes = [_HDC]
        g.CreateCompatibleDC.restype = _HDC
        g.CreateCompatibleBitmap.argtypes = [_HDC, ctypes.c_int, ctypes.c_int]
        g.CreateCompatibleBitmap.restype = _HBITMAP
        g.SelectObject.argtypes = [_HDC, _HGDIOBJ]
        g.SelectObject.restype = _HGDIOBJ
        g.DeleteObject.argtypes = [_HGDIOBJ]
        g.DeleteDC.argtypes = [_HDC]
        g.GetDIBits.argtypes = [
            _HDC, _HBITMAP, _UINT, _UINT,
            ctypes.c_void_p, ctypes.c_void_p, _UINT,
        ]
        g.GetDIBits.restype = ctypes.c_int

    # -- lookups -------------------------------------------------------------

    def window_text(self, hwnd) -> str:
        n = self.user32.GetWindowTextLengthW(hwnd)
        if n <= 0:
            return ""
        buf = ctypes.create_unicode_buffer(n + 2)
        self.user32.GetWindowTextW(hwnd, buf, n + 2)
        return buf.value

    def is_iconic(self, hwnd) -> bool:
        """True when the window is minimized (no client bitmap is kept)."""
        return bool(self.user32.IsIconic(hwnd))

    def class_name(self, hwnd) -> str:
        buf = ctypes.create_unicode_buffer(256)
        self.user32.GetClassNameW(hwnd, buf, 256)
        return buf.value

    def client_size(self, hwnd) -> tuple[int, int]:
        rect = _RECT()
        if not self.user32.GetClientRect(hwnd, ctypes.byref(rect)):
            return 0, 0
        return rect.right - rect.left, rect.bottom - rect.top

    def child_rect_in_parent(self, child, parent) -> Optional[tuple[int, int, int, int]]:
        """Child window rect expressed in the parent's client coordinates."""
        rect = _RECT()
        if not self.user32.GetWindowRect(child, ctypes.byref(rect)):
            return None
        # NULL source handle = screen coordinates -> parent client coordinates
        if not self.user32.MapWindowPoints(None, parent, ctypes.byref(rect), 2):
            return None
        return rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top

    def _surface_child(self, parent) -> Optional[int]:
        """Find the child window that hosts the guest framebuffer.

        Do not require ``IsWindowVisible``: MuMu clears WS_VISIBLE on the
        render surface in several window states (it composites the child
        through Qt) while still painting it for PrintWindow. Visibility is
        only used to prefer one candidate over another.
        """
        found: list[tuple[int, bool]] = []

        def visit(hwnd, _):
            cls = self.class_name(hwnd).lower()
            title = self.window_text(hwnd).lower()
            if cls in _SURFACE_CLASSES or any(t in title for t in _SURFACE_TITLES):
                width, height = self.client_size(hwnd)
                if width > 100 and height > 100:
                    found.append((hwnd, bool(self.user32.IsWindowVisible(hwnd))))
            return True

        self.user32.EnumChildWindows(parent, self._enum_proc(visit), 0)
        if not found:
            return None
        visible = [hwnd for hwnd, vis in found if vis]
        return visible[0] if visible else found[0][0]

    def find_surface(self, title_hint: str = "") -> Optional[tuple[int, int, str]]:
        """Return (surface_hwnd, top_hwnd, top_title) for the MuMu instance."""
        tops: list[tuple[int, str]] = []

        def visit(hwnd, _):
            if self.user32.IsWindowVisible(hwnd):
                tops.append((hwnd, self.window_text(hwnd)))
            return True

        self.user32.EnumWindows(self._enum_proc(visit), 0)

        candidates: list[tuple[int, int, str]] = []
        for top, title in tops:
            child = self._surface_child(top)
            if child is None:
                continue
            # The multi-instance manager ("MuMu模拟器") wraps a tiny preview
            # child (~192x108). Binding to it silently yields a useless stream,
            # so only windows big enough to be a real guest display qualify.
            width, height = self.client_size(child)
            if width < MIN_SURFACE_W or height < MIN_SURFACE_H:
                continue
            candidates.append((child, top, title))

        if not candidates:
            return None

        hint = (title_hint or "").strip().lower()
        if hint:
            matched = [c for c in candidates if hint in c[2].lower()]
            if matched:
                candidates = matched
            else:
                print(
                    f"[mumu-capture] no window title contains {title_hint!r}; "
                    f"candidates: {[c[2] for c in candidates]}",
                    file=sys.stderr,
                )
        if len(candidates) > 1:
            # Prefer the instance window over the manager's preview pane, and
            # keep the choice stable across reconnects (enumeration order is
            # not stable while windows are being shown/hidden).
            def rank(item: tuple[int, int, str]) -> int:
                text = item[2].lower()
                if "安卓设备" in text or "android" in text:
                    return 0
                if text.startswith("mumu模拟器") or text.startswith("mumu player"):
                    return 2
                return 1

            candidates.sort(key=rank)
            print(
                f"[mumu-capture] {len(candidates)} MuMu windows found "
                f"{[c[2] for c in candidates]}; using {candidates[0][2]!r} "
                f"(set 'windowTitle' in capture_config.json to choose)",
                file=sys.stderr,
            )
        return candidates[0]


class _BitBuffer:
    """Reusable memory DC + DIB buffer for one window/size."""

    def __init__(self, api: _WinApi) -> None:
        self.api = api
        self.hwnd = 0
        self.width = 0
        self.height = 0
        self._hdc = 0
        self._mdc = 0
        self._bmp = 0
        self._buf = None
        self._bih = None

    def attach(self, hwnd: int) -> None:
        if hwnd != self.hwnd:
            self.release()
            self.hwnd = hwnd

    def release(self) -> None:
        u, g = self.api.user32, self.api.gdi32
        if self._mdc:
            try:
                g.DeleteDC(self._mdc)
            except Exception:  # noqa: BLE001
                pass
        if self._bmp:
            try:
                g.DeleteObject(self._bmp)
            except Exception:  # noqa: BLE001
                pass
        if self._hdc and self.hwnd:
            try:
                u.ReleaseDC(self.hwnd, self._hdc)
            except Exception:  # noqa: BLE001
                pass
        self._mdc = self._bmp = self._hdc = 0
        self._buf = None
        self.width = self.height = 0

    def _ensure(self, width: int, height: int) -> bool:
        if self._mdc and (width, height) == (self.width, self.height):
            return True
        self.release()
        if not self.hwnd:
            return False
        u, g = self.api.user32, self.api.gdi32
        self._hdc = u.GetDC(self.hwnd)
        if not self._hdc:
            return False
        self._mdc = g.CreateCompatibleDC(self._hdc)
        self._bmp = g.CreateCompatibleBitmap(self._hdc, width, height)
        if not self._mdc or not self._bmp:
            return False
        g.SelectObject(self._mdc, self._bmp)
        self._buf = ctypes.create_string_buffer(width * height * 4)

        bih = _BITMAPINFOHEADER()
        bih.biSize = ctypes.sizeof(_BITMAPINFOHEADER)
        bih.biWidth = width
        bih.biHeight = -height  # negative -> top-down rows, no manual flip
        bih.biPlanes = 1
        bih.biBitCount = 32
        bih.biCompression = 0
        self._bih = bih
        self.width, self.height = width, height
        return True

    def grab(self) -> Optional[np.ndarray]:
        if not self.hwnd or not self.api.user32.IsWindow(self.hwnd):
            return None
        width, height = self.api.client_size(self.hwnd)
        if width <= 0 or height <= 0:
            return None
        if not self._ensure(width, height):
            return None
        if not self.api.user32.PrintWindow(self.hwnd, self._mdc, _PW_RENDERFULLCONTENT):
            return None
        got = self.api.gdi32.GetDIBits(
            self._mdc, self._bmp, 0, height, self._buf, ctypes.byref(self._bih), 0
        )
        if got != height:
            return None
        raw = np.frombuffer(self._buf.raw, dtype=np.uint8).reshape(height, width, 4)
        return raw[:, :, :3].copy()  # BGRA -> BGR (row 0 is the top row)


_win_api: Optional[_WinApi] = None


def _winapi() -> _WinApi:
    global _win_api
    if _win_api is None:
        _win_api = _WinApi()
    return _win_api


def is_blank(frame: Optional[np.ndarray], threshold: float = 2.0) -> bool:
    """True when the frame carries no visible content.

    Mean brightness is the right signal, not variance: a secure layer is not
    always *exactly* zero. FGO's in-game frames measured mean 0.1 / std 3.1
    (a handful of surviving HUD pixels), which a mean+std test would have
    waved through as a usable frame.
    """
    if frame is None or frame.size == 0:
        return True
    flat = frame.reshape(-1, frame.shape[2]) if frame.ndim == 3 else frame.reshape(-1, 1)
    return float(flat.mean()) < threshold


# ---------------------------------------------------------------------------
# stream
# ---------------------------------------------------------------------------


class MuMuStream:
    """Publishes BGR frames from MuMu, like ScrcpyStream does.

    ``source``: "auto" (IPC, then window capture when IPC is blank),
    "ipc", "window", or "scrcpy" (not handled here).
    """

    def __init__(
        self,
        mumu_root: Optional[str] = None,
        vm_index: int = 1,
        package: Optional[str] = None,
        target_fps: float = 12.0,
        max_size: int = 0,
        flip_vertical: bool = True,
        source: str = "auto",
        window_title: str = "",
        native_size: Optional[tuple[int, int]] = None,
        blank_switch_frames: int = 18,
        window_wait_ms: int = 12000,
        adb_path: str = "",
        serial: str = "",
    ) -> None:
        resolved_root = resolve_root(mumu_root)
        if mumu_root and resolved_root != mumu_root:
            print(
                f"[mumu-capture] configured mumu root {mumu_root!r} has no renderer DLL; "
                f"using {resolved_root!r}",
                file=sys.stderr,
            )
        elif not mumu_root and resolved_root != DEFAULT_ROOT:
            print(
                f"[mumu-capture] mumu root auto-detected: {resolved_root!r}",
                file=sys.stderr,
            )
        self.mumu_root = resolved_root
        self.vm_index = int(vm_index)
        self.package = package or DEFAULT_PACKAGE
        self.target_fps = max(1.0, float(target_fps))
        self.max_size = int(max_size or 0)
        self.flip_vertical = bool(flip_vertical)
        self.source = str(source or "auto").lower()
        self.window_title = window_title or ""
        self.native_size = tuple(native_size) if native_size else None
        self.blank_switch_frames = max(1, int(blank_switch_frames))
        self.window_wait_ms = max(0, int(window_wait_ms))
        self.adb_path = adb_path or ""
        self.serial = serial or ""

        self._api: Optional[_NemuApi] = None
        self._handle = 0
        self._display_id = 0
        self._buffer: Optional[ctypes.Array] = None
        self._raw_w = 0
        self._raw_h = 0

        self._mode: Optional[str] = None
        self._painter: Optional[_BitBuffer] = None
        self._surface_hwnd = 0
        self._top_hwnd = 0
        self._paint_parent = False
        self._iconic_notice = False
        self._last_ipc_retry = 0.0
        self._latest_frame: Optional[np.ndarray] = None
        self._latest_lock = threading.Lock()
        self._frame_id = 0
        self._decoded_at_ms: Optional[int] = None

        self._thread: Optional[threading.Thread] = None
        self._stop_event = threading.Event()
        self._dead_reason: Optional[str] = None

        self.width = 0
        self.height = 0
        self.device_name = "mumu"

    # -- lifecycle -----------------------------------------------------------

    def _mode_order(self) -> list[str]:
        if self.source == "ipc":
            return ["ipc"]
        if self.source == "window":
            return ["window"]
        return ["ipc", "window"]

    def start(self) -> tuple[int, int]:
        problems: list[str] = []
        for mode in self._mode_order():
            try:
                probe = self._open(mode)
            except Exception as exc:  # noqa: BLE001
                problems.append(f"{mode}: {exc}")
                print(f"[mumu-capture] {mode} unavailable ({exc})", file=sys.stderr)
                self._teardown()
                continue

            if mode == "ipc" and is_blank(probe):
                problems.append("ipc: frame is blank (FLAG_SECURE layer)")
                print(
                    "[mumu-capture] renderer IPC returned a blank frame; "
                    "MuMu drops secure layers, trying window capture",
                    file=sys.stderr,
                )
                self._teardown()
                continue

            self._mode = mode
            self._publish(probe)
            self._stop_event.clear()
            self._dead_reason = None
            self._thread = threading.Thread(
                target=self._capture_loop, name=f"mumu-{mode}", daemon=True
            )
            self._thread.start()
            print(
                f"[mumu-capture] {mode} mode @ {self.width}x{self.height} "
                f"({self.target_fps:.0f} fps)",
                file=sys.stderr,
            )
            return self.width, self.height

        raise RuntimeError("no usable MuMu frame source: " + "; ".join(problems))

    def _open(self, mode: str) -> Optional[np.ndarray]:
        """Connect one backend and return its first converted frame."""
        if mode == "ipc":
            _isolate_native_stdout()
            self._connect_ipc()
            frame = self._grab_ipc()
        else:
            frame = self._open_window_retrying()
            if frame is None:
                raise RuntimeError("first frame unavailable")
            return frame
        if frame is None:
            raise RuntimeError("first frame unavailable")
        return frame

    def _open_window_retrying(self, wait_ms: Optional[float] = None) -> Optional[np.ndarray]:
        """Attach to MuMu's window, retrying while the instance boots.

        Mash is often started before (or while) the emulator instance comes
        up, and MuMu recreates its render window on every instance launch.
        Bailing out on the first miss would drop us onto scrcpy and back to a
        black screen, so poll for a bounded window instead.
        """
        budget = (self.window_wait_ms if wait_ms is None else wait_ms) / 1000.0
        deadline = time.monotonic() + max(0.0, budget)
        last: object = "window not found"
        announced = False
        while True:
            try:
                self._attach_window()
                frame = self._grab_window()
                if frame is not None:
                    return frame
                last = "window painted nothing"
            except Exception as exc:  # noqa: BLE001
                last = exc
            if time.monotonic() >= deadline:
                raise RuntimeError(f"MuMu window unusable after {budget:.0f}s ({last})")
            if not announced:
                print(
                    f"[mumu-capture] waiting up to {budget:.0f}s for MuMu's window ({last})",
                    file=sys.stderr,
                )
                announced = True
            time.sleep(0.4)

    def stop(self) -> None:
        self._stop_event.set()
        if self._thread is not None and self._thread.is_alive():
            self._thread.join(timeout=1.5)
        self._thread = None
        self._teardown()

    def _teardown(self) -> None:
        self._disconnect_ipc()
        if self._painter is not None:
            self._painter.release()
            self._painter = None
        self._surface_hwnd = 0
        self._top_hwnd = 0
        self._mode = None

    # -- ipc backend ---------------------------------------------------------

    def _connect_ipc(self) -> None:
        self._api = _NemuApi(self.mumu_root)
        print(f"[mumu-capture] dll: {self._api.path}", file=sys.stderr)

        handle = self._api.connect(self.mumu_root, self.vm_index)
        if not handle:
            # MuMu hands out instance slots per launch and a restarted instance
            # often lands on a different index, so the configured one goes
            # stale. Walk the slots and take the first that answers.
            for index in range(0, 16):
                if index == self.vm_index:
                    continue
                candidate = self._api.connect(self.mumu_root, index)
                if candidate:
                    print(
                        f"[mumu-capture] instance index {self.vm_index} not live; "
                        f"using {index}",
                        file=sys.stderr,
                    )
                    self.vm_index = index
                    handle = candidate
                    break
        if not handle:
            raise RuntimeError(
                f"nemu_connect failed for every instance index (configured {self.vm_index})"
            )
        self._handle = handle

        display_id = self._api.get_display_id(handle, self.package.encode("utf-8"), 0)
        if display_id < 0:
            print(f"[mumu-capture] get_display_id failed ({display_id}); using 0", file=sys.stderr)
            display_id = 0
        self._display_id = display_id

        width = ctypes.c_int(0)
        height = ctypes.c_int(0)
        ret = self._api.capture_display(
            handle, display_id, 0, ctypes.byref(width), ctypes.byref(height), None
        )
        if ret != 0 or width.value <= 0 or height.value <= 0:
            raise RuntimeError(f"capture probe failed (ret={ret}, {width.value}x{height.value})")

        self._raw_w = width.value
        self._raw_h = height.value
        self._buffer = ctypes.create_string_buffer(self._raw_w * self._raw_h * 4)
        self.width, self.height = self._target_size(self._raw_w, self._raw_h)

    def _disconnect_ipc(self) -> None:
        if self._api is not None and self._handle:
            try:
                self._api.disconnect(self._handle)
            except Exception:  # noqa: BLE001
                pass
        self._handle = 0
        self._buffer = None

    def _grab_ipc(self) -> Optional[np.ndarray]:
        assert self._api is not None and self._buffer is not None
        width = ctypes.c_int(self._raw_w)
        height = ctypes.c_int(self._raw_h)
        ret = self._api.capture_display(
            self._handle,
            self._display_id,
            len(self._buffer),
            ctypes.byref(width),
            ctypes.byref(height),
            ctypes.cast(self._buffer, ctypes.c_void_p),
        )
        if ret != 0:
            return None
        width_v = width.value or self._raw_w
        height_v = height.value or self._raw_h
        need = width_v * height_v * 4
        if need > len(self._buffer):
            return None

        raw = np.frombuffer(self._buffer, dtype=np.uint8, count=need).reshape(height_v, width_v, 4)
        frame = cv2.cvtColor(raw, cv2.COLOR_RGBA2BGR)
        if self.flip_vertical:
            frame = cv2.flip(frame, 0)
        if (width_v, height_v) != (self.width, self.height):
            frame = self._fit(frame)
        return frame

    # -- window backend ------------------------------------------------------

    def _attach_window(self) -> None:
        found = _winapi().find_surface(self.window_title)
        if not found:
            raise RuntimeError(
                "no MuMu window with a render surface on screen "
                "(instance not started, or window closed)"
            )
        surface, top, title = found
        self._surface_hwnd = surface
        self._top_hwnd = top
        # Paint the render child directly: it is exactly the 16:9 guest area,
        # no chrome to crop. If MuMu ever clears WS_VISIBLE on it (which makes
        # PrintWindow refuse), fall back to painting the top-level window and
        # cutting the child's rectangle out of it.
        self._paint_parent = False
        painter = _BitBuffer(_winapi())
        painter.attach(surface)
        self._painter = painter

        cap_w, cap_h = _winapi().client_size(surface)
        if cap_w < MIN_SURFACE_W or cap_h < MIN_SURFACE_H:
            raise RuntimeError(
                f"window {title!r} has a {cap_w}x{cap_h} client rect "
                "(instance manager preview, not a guest display)"
            )

        native = (
            self.native_size
            or self._probe_native_size()
            or self._probe_native_size_adb()
        )
        self.width, self.height = self._target_size(*(native or (cap_w, cap_h)))
        print(
            f"[mumu-capture] window {title!r} {cap_w}x{cap_h} -> output {self.width}x{self.height}",
            file=sys.stderr,
        )

    def _probe_native_size(self) -> Optional[tuple[int, int]]:
        """Ask the renderer for the guest resolution so the window-scaled
        capture can be resized back to device pixels and CV scale stays put."""
        try:
            _isolate_native_stdout()
            api = _NemuApi(self.mumu_root)
            indices = [self.vm_index] + [i for i in range(0, 16) if i != self.vm_index]
            for index in indices:
                handle = api.connect(self.mumu_root, index)
                if not handle:
                    continue
                try:
                    display_id = api.get_display_id(handle, self.package.encode("utf-8"), 0)
                    if display_id < 0:
                        display_id = 0
                    width = ctypes.c_int(0)
                    height = ctypes.c_int(0)
                    ret = api.capture_display(
                        handle, display_id, 0, ctypes.byref(width), ctypes.byref(height), None
                    )
                    if ret == 0 and width.value > 0 and height.value > 0:
                        return width.value, height.value
                finally:
                    api.disconnect(handle)
        except Exception as exc:  # noqa: BLE001
            print(f"[mumu-capture] native size probe failed ({exc})", file=sys.stderr)
        return None

    def _probe_native_size_adb(self) -> Optional[tuple[int, int]]:
        """Last resort for the guest resolution: ask the device itself."""
        if not self.adb_path or not self.serial:
            return None
        try:
            import subprocess

            out = subprocess.run(
                [self.adb_path, "-s", self.serial, "shell", "wm", "size"],
                capture_output=True, text=True, timeout=6,
                encoding="utf-8", errors="replace",
            ).stdout
            for line in out.splitlines():
                if "size:" in line and "x" in line:
                    dims = line.split("size:")[-1].strip().split("x")
                    width, height = int(dims[0]), int(dims[1].split()[0])
                    if width > 0 and height > 0:
                        return width, height
        except Exception as exc:  # noqa: BLE001
            print(f"[mumu-capture] adb size probe failed ({exc})", file=sys.stderr)
        return None

    def _grab_window(self) -> Optional[np.ndarray]:
        frame = self._paint_once()
        if frame is None and not self._paint_parent:
            # The render child stopped accepting PrintWindow (hidden surface).
            # Repaint through the top-level window and crop it instead.
            print(
                "[mumu-capture] render surface refused PrintWindow; "
                "painting the top-level window instead",
                file=sys.stderr,
            )
            self._paint_parent = True
            if self._painter is not None:
                self._painter.release()
            frame = self._paint_once()
        if frame is None:
            return None
        if frame.shape[1] != self.width or frame.shape[0] != self.height:
            frame = self._fit(frame)
        return frame

    def _paint_once(self) -> Optional[np.ndarray]:
        if self._painter is None:
            return None
        api = _winapi()
        target = self._top_hwnd if self._paint_parent else self._surface_hwnd
        if not target:
            return None
        if self._painter.hwnd != target:
            self._painter.attach(target)
        frame = self._painter.grab()
        if frame is None or not self._paint_parent:
            return frame
        rect = api.child_rect_in_parent(self._surface_hwnd, self._top_hwnd)
        if not rect:
            return None
        x, y, w, h = rect
        height, width = frame.shape[:2]
        if x < 0 or y < 0 or x + w > width or y + h > height or w <= 0 or h <= 0:
            return None
        return np.ascontiguousarray(frame[y:y + h, x:x + w])

    # -- shared --------------------------------------------------------------

    def _target_size(self, width: int, height: int) -> tuple[int, int]:
        if self.max_size and max(width, height) > self.max_size:
            scale = self.max_size / float(max(width, height))
            return max(1, int(round(width * scale))), max(1, int(round(height * scale)))
        return width, height

    def _fit(self, frame: np.ndarray) -> np.ndarray:
        interpolation = cv2.INTER_AREA
        if self.width > frame.shape[1]:
            interpolation = cv2.INTER_CUBIC
        return cv2.resize(frame, (self.width, self.height), interpolation=interpolation)

    def _grab(self) -> Optional[np.ndarray]:
        if self._mode == "window":
            return self._grab_window()
        return self._grab_ipc()

    # -- capture loop --------------------------------------------------------

    def _capture_loop(self) -> None:
        interval = 1.0 / self.target_fps
        failures = 0
        blanks = 0
        while not self._stop_event.is_set():
            # A minimized window has no client bitmap, so PrintWindow returns
            # nothing and we would march straight into "30 failures" death.
            # That is not a capture fault: handle it before touching a backend.
            if self._mode == "window" and self._minimized():
                if self._window_to_ipc():
                    failures = 0
                    blanks = 0
                    continue
                if not self._iconic_notice:
                    print(
                        "[mumu-capture] MuMu window is minimized and the renderer IPC "
                        "is unusable here; waiting for it to be restored",
                        file=sys.stderr,
                    )
                    self._iconic_notice = True
                self._stop_event.wait(0.5)
                continue
            self._iconic_notice = False
            started = time.perf_counter()
            try:
                frame = self._grab()
            except Exception as exc:  # noqa: BLE001
                self._dead_reason = f"capture raised: {exc}"
                print(f"[mumu-capture] {self._dead_reason}", file=sys.stderr)
                return
            if frame is None:
                failures += 1
                # The emulator may have been restarted or its window recreated
                # under us: the handle goes stale while the process lives on.
                if failures in (10, 20):
                    if self._reconnect():
                        print("[mumu-capture] reconnected after capture failures", file=sys.stderr)
                        failures = 0
                        continue
                if failures >= 30:
                    self._dead_reason = "capture failed 30 times in a row"
                    print(f"[mumu-capture] {self._dead_reason}", file=sys.stderr)
                    return
            else:
                failures = 0
                self._publish(frame)
                # FLAG_SECURE is not on all the time: FGO's title screen comes
                # through the renderer IPC, the in-game screens do not. Watch
                # for the frame going black mid-session and hop onto window
                # capture without dropping the stream.
                if self._mode == "ipc" and "window" in self._mode_order():
                    blanks = blanks + 1 if is_blank(frame) else 0
                    if blanks >= self.blank_switch_frames:
                        blanks = 0
                        if self._switch_to_window():
                            print(
                                "[mumu-capture] game frame went black on IPC "
                                "(secure layer); switched to window capture",
                                file=sys.stderr,
                            )
                            continue
            delay = interval - (time.perf_counter() - started)
            if delay > 0:
                time.sleep(delay)

    def _minimized(self) -> bool:
        top = self._top_hwnd
        if not top:
            return False
        try:
            return _winapi().is_iconic(top)
        except Exception:  # noqa: BLE001
            return False

    def _window_to_ipc(self) -> bool:
        """Use the renderer IPC while the window is minimized.

        MuMu keeps rendering into its own renderer when the window is iconified,
        so IPC normally still delivers live frames - it only goes blank when the
        guest turned FLAG_SECURE on. In that case stay on window capture and wait
        for the user to restore the window.
        """
        now = time.monotonic()
        if now - self._last_ipc_retry < 5.0:
            return False
        self._last_ipc_retry = now
        try:
            self._teardown()
            probe = self._open("ipc")
        except Exception as exc:  # noqa: BLE001
            print(
                f"[mumu-capture] IPC retry while minimized failed ({exc})",
                file=sys.stderr,
            )
            self._back_to_window()
            return False
        if probe is None or is_blank(probe):
            self._disconnect_ipc()
            self._back_to_window()
            return False
        self._mode = "ipc"
        self._publish(probe)
        print(
            "[mumu-capture] window minimized; continuing on renderer IPC",
            file=sys.stderr,
        )
        return True

    def _back_to_window(self) -> None:
        self._mode = "window"
        try:
            self._attach_window()
        except Exception:  # noqa: BLE001
            pass

    def _switch_to_window(self) -> bool:
        """Re-open the same stream through PrintWindow (called from the loop)."""
        backup_ipc = self._mode == "ipc"
        try:
            self._teardown()
            probe = self._open("window")
        except Exception as exc:  # noqa: BLE001
            print(f"[mumu-capture] window capture switch failed ({exc})", file=sys.stderr)
            if backup_ipc:
                try:
                    self._open("ipc")
                    self._mode = "ipc"
                except Exception:  # noqa: BLE001
                    pass
            return False
        self._mode = "window"
        if probe is not None:
            self._publish(probe)
        return True

    def _reconnect(self) -> bool:
        try:
            if self._mode == "window":
                self._attach_window()
                return self._grab_window() is not None

            self._disconnect_ipc()
            self._connect_ipc()
            return self._grab_ipc() is not None
        except Exception:  # noqa: BLE001
            return False

    def _publish(self, bgr: np.ndarray) -> None:
        with self._latest_lock:
            self._latest_frame = bgr
            self._frame_id += 1
            self._decoded_at_ms = int(time.time() * 1000)

    # -- frame access (ScrcpyStream-compatible surface) ----------------------

    def get_latest_snapshot(self):
        with self._latest_lock:
            if self._latest_frame is None:
                return None, None
            image = self._latest_frame.copy()
            return image, {
                "source": f"mumu-{self._mode or self.source}",
                "frameId": self._frame_id,
                "decodedAtMs": self._decoded_at_ms,
                "width": image.shape[1],
                "height": image.shape[0],
            }

    def get_latest_bgr(self) -> Optional[np.ndarray]:
        with self._latest_lock:
            if self._latest_frame is None:
                return None
            return self._latest_frame.copy()

    def wait_for_frame(self, timeout: float = 5.0) -> Optional[np.ndarray]:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            frame = self.get_latest_bgr()
            if frame is not None:
                return frame
            if not self.is_decoder_alive():
                return None
            time.sleep(0.02)
        return self.get_latest_bgr()

    def get_latest_jpeg(self, quality: int = 85, wait: float = 0.0) -> Optional[bytes]:
        frame = self.wait_for_frame(timeout=wait) if wait > 0 else self.get_latest_bgr()
        if frame is None:
            return None
        ok, buf = cv2.imencode(".jpg", frame, [int(cv2.IMWRITE_JPEG_QUALITY), quality])
        return buf.tobytes() if ok else None

    def get_latest_png(self, wait: float = 0.0) -> Optional[bytes]:
        frame = self.wait_for_frame(timeout=wait) if wait > 0 else self.get_latest_bgr()
        if frame is None:
            return None
        ok, buf = cv2.imencode(".png", frame)
        return buf.tobytes() if ok else None

    def is_decoder_alive(self) -> bool:
        return self._thread is not None and self._thread.is_alive()

    # -- touch ---------------------------------------------------------------

    def send_touch(self, cmd: dict) -> None:
        """No scrcpy control channel exists on this frame source.

        Upstream added ``send_touch`` for the scrcpy control socket. MuMu's
        capture paths here are video-only, so input keeps going through adb.
        Raise the error type ``_scrcpy_touch`` catches: the command fails
        loudly instead of the sidecar dying with an AttributeError, and the
        shared recognition process stays alive.
        """
        raise RuntimeError(
            "mumu frame source has no scrcpy control channel; input must stay on adb"
        )
