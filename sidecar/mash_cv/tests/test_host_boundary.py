"""Keep OS detection and native APIs out of shared CV/streaming code."""

import ast
from pathlib import Path

PACKAGE = Path(__file__).parents[1] / "mash_cv"
HOST_DEPENDENCIES = {
    "sys.platform", "os.name", "os.uname", "ctypes.windll", "ctypes.WinDLL",
    "subprocess.CREATE_NO_WINDOW", "subprocess.STARTUPINFO",
    "platform.system", "platform.machine", "platform.platform",
}


def host_dependencies(source):
    tree = ast.parse(source)
    return {
        ast.unparse(node) for node in ast.walk(tree)
        if isinstance(node, ast.Attribute) and ast.unparse(node) in HOST_DEPENDENCIES
    }


def test_shared_sidecar_modules_have_no_os_dependencies():
    violations = {}
    for path in PACKAGE.rglob("*.py"):
        if "host" in path.relative_to(PACKAGE).parts:
            continue
        dependencies = host_dependencies(path.read_text(encoding="utf-8"))
        if dependencies:
            violations[str(path.relative_to(PACKAGE))] = dependencies
    assert not violations, f"Move OS dependencies into mash_cv/host/: {violations}"


def test_boundary_check_detects_native_process_and_os_checks():
    assert host_dependencies('if sys.platform == "win32":\n    flags = subprocess.CREATE_NO_WINDOW') == {
        "sys.platform", "subprocess.CREATE_NO_WINDOW",
    }
    assert host_dependencies("api = ctypes.windll.kernel32") == {"ctypes.windll"}
    assert not host_dependencies("host_process.hidden_process_options()")
