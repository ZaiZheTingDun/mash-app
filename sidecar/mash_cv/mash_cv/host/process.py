"""Console-free child processes and frozen-runtime isolation on Windows."""

from contextlib import contextmanager
import ctypes
import os
import subprocess
import sys
import threading


_dll_lock = threading.Lock()


def hidden_process_options() -> dict:
    if sys.platform == "win32":
        return {"creationflags": subprocess.CREATE_NO_WINDOW}
    return {}


def clean_external_env() -> dict[str, str]:
    env = os.environ.copy()
    for key in ("DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "LD_LIBRARY_PATH"):
        original = env.pop(f"{key}_ORIG", None)
        if original is None:
            env.pop(key, None)
        else:
            env[key] = original
    bundle = getattr(sys, "_MEIPASS", None)
    if sys.platform == "win32" and bundle:
        root = os.path.normcase(os.path.abspath(bundle))
        env["PATH"] = os.pathsep.join(
            part for part in env.get("PATH", "").split(os.pathsep)
            if not (os.path.normcase(os.path.abspath(part)) == root
                    or os.path.normcase(os.path.abspath(part)).startswith(root + os.sep))
        )
    return env


def _get_dll_directory() -> str:
    api = ctypes.windll.kernel32.GetDllDirectoryW
    length = api(0, None)
    buffer = ctypes.create_unicode_buffer(length + 1)
    api(len(buffer), buffer)
    return buffer.value


def _set_dll_directory(directory: str | None) -> None:
    api = ctypes.windll.kernel32.SetDllDirectoryW
    api.argtypes = [ctypes.c_wchar_p]
    if not api(directory):
        raise ctypes.WinError()


@contextmanager
def external_process_scope():
    # Only external tools (ADB) need the host DLL search path. OCR workers
    # must retain the frozen runtime's DLL directory. Restore it when this
    # scope exits, including when the external process fails.
    if sys.platform != "win32" or not getattr(sys, "frozen", False):
        yield
        return
    with _dll_lock:
        previous = _get_dll_directory()
        _set_dll_directory(None)
        try:
            yield
        finally:
            _set_dll_directory(previous or None)
