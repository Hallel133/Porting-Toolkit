from pathlib import Path

import pytest

FIXTURES = Path(__file__).resolve().parents[2] / "fixtures" / "games"


@pytest.fixture
def games() -> Path:
    return FIXTURES
