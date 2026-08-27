"""Deterministic exception fixture for Rich traceback conformance.

This file is kept as a simple, stable traceback generator for manual reference.
The fixture generator currently constructs `Traceback` instances from explicit
frames and does not execute this module.
"""


def level3() -> None:
    1 / 0  # noqa: B018


def level2() -> None:
    level3()


def level1() -> None:
    level2()
