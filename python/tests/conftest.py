"""
pytest configuration and fixtures for int2dds tests.
"""

import pytest


@pytest.fixture
def domain_id() -> int:
    """Return a domain ID for testing."""
    return 0


@pytest.fixture
def topic_name() -> str:
    """Return a topic name for testing."""
    return "pytest_test_topic"
