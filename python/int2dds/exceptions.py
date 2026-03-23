"""
DDS exception hierarchy for int2dds.

Maps FFI return codes to Python exceptions.
"""

from __future__ import annotations


class DdsError(Exception):
    """Base class for all DDS errors."""

    def __init__(self, message: str = "", code: int = 1) -> None:
        self.code = code
        super().__init__(message or f"DDS operation failed with code {code}")


class DdsTimeout(DdsError):
    """Operation timed out."""

    def __init__(self, message: str = "Operation timed out") -> None:
        super().__init__(message, code=2)


class DdsUnsupported(DdsError):
    """Operation not supported."""

    def __init__(self, message: str = "Operation not supported") -> None:
        super().__init__(message, code=3)


class DdsInvalidArgument(DdsError):
    """Invalid argument provided."""

    def __init__(self, message: str = "Invalid argument") -> None:
        super().__init__(message, code=11)


class DdsBadAlloc(DdsError):
    """Memory allocation failed."""

    def __init__(self, message: str = "Bad allocation") -> None:
        super().__init__(message, code=10)


class DdsAlreadyDeleted(DdsError):
    """Entity was already deleted."""

    def __init__(self, message: str = "Already deleted") -> None:
        super().__init__(message, code=20)


class DdsNotEnabled(DdsError):
    """Entity not enabled."""

    def __init__(self, message: str = "Entity not enabled") -> None:
        super().__init__(message, code=21)


class DdsImmutablePolicy(DdsError):
    """Attempted to modify immutable QoS policy."""

    def __init__(self, message: str = "Immutable policy") -> None:
        super().__init__(message, code=22)


class DdsInconsistentPolicy(DdsError):
    """Inconsistent QoS policy combination."""

    def __init__(self, message: str = "Inconsistent policy") -> None:
        super().__init__(message, code=23)


class DdsPreconditionNotMet(DdsError):
    """Precondition not met."""

    def __init__(self, message: str = "Precondition not met") -> None:
        super().__init__(message, code=24)


class DdsOutOfResources(DdsError):
    """Out of resources."""

    def __init__(self, message: str = "Out of resources") -> None:
        super().__init__(message, code=25)


class DdsIllegalOperation(DdsError):
    """Illegal operation."""

    def __init__(self, message: str = "Illegal operation") -> None:
        super().__init__(message, code=26)


class DdsNoData(DdsError):
    """No data available."""

    def __init__(self, message: str = "No data available") -> None:
        super().__init__(message, code=27)


class DdsNullPointer(DdsError):
    """Null pointer passed to FFI."""

    def __init__(self, message: str = "Null pointer") -> None:
        super().__init__(message, code=100)


# Return code constants (matching int2dds-ffi.h)
INT2DDS_RET_OK = 0
INT2DDS_RET_ERROR = 1
INT2DDS_RET_TIMEOUT = 2
INT2DDS_RET_UNSUPPORTED = 3
INT2DDS_RET_BAD_ALLOC = 10
INT2DDS_RET_INVALID_ARGUMENT = 11
INT2DDS_RET_ALREADY_DELETED = 20
INT2DDS_RET_NOT_ENABLED = 21
INT2DDS_RET_IMMUTABLE_POLICY = 22
INT2DDS_RET_INCONSISTENT_POLICY = 23
INT2DDS_RET_PRECONDITION_NOT_MET = 24
INT2DDS_RET_OUT_OF_RESOURCES = 25
INT2DDS_RET_ILLEGAL_OPERATION = 26
INT2DDS_RET_NO_DATA = 27
INT2DDS_RET_NULL_POINTER = 100

# Exception mapping
_EXCEPTION_MAP: dict[int, type[DdsError]] = {
    INT2DDS_RET_ERROR: DdsError,
    INT2DDS_RET_TIMEOUT: DdsTimeout,
    INT2DDS_RET_UNSUPPORTED: DdsUnsupported,
    INT2DDS_RET_BAD_ALLOC: DdsBadAlloc,
    INT2DDS_RET_INVALID_ARGUMENT: DdsInvalidArgument,
    INT2DDS_RET_ALREADY_DELETED: DdsAlreadyDeleted,
    INT2DDS_RET_NOT_ENABLED: DdsNotEnabled,
    INT2DDS_RET_IMMUTABLE_POLICY: DdsImmutablePolicy,
    INT2DDS_RET_INCONSISTENT_POLICY: DdsInconsistentPolicy,
    INT2DDS_RET_PRECONDITION_NOT_MET: DdsPreconditionNotMet,
    INT2DDS_RET_OUT_OF_RESOURCES: DdsOutOfResources,
    INT2DDS_RET_ILLEGAL_OPERATION: DdsIllegalOperation,
    INT2DDS_RET_NO_DATA: DdsNoData,
    INT2DDS_RET_NULL_POINTER: DdsNullPointer,
}


def check_ret(ret: int) -> None:
    """Check FFI return code and raise appropriate exception."""
    if ret == INT2DDS_RET_OK:
        return
    exc_class = _EXCEPTION_MAP.get(ret, DdsError)
    raise exc_class()
