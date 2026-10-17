import sys

import pytest


def unit():
    sys.exit(pytest.main(["-v", "tests/test_cv.py"]))


def integration():
    sys.exit(pytest.main(["-v", "tests/test_integration.py"]))
