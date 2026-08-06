"""
Base protocol for DDS data types.

IDL-generated types must implement this protocol to be usable with int2dds.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, ClassVar, Protocol, TypeVar

if TYPE_CHECKING:
    from int2dds.cdr.writer import Extensibility

T = TypeVar("T", bound="DdsType")


class DdsType(Protocol):
    """
    Protocol for DDS data types.

    IDL-generated types must have:
    - _dds_type_name: The DDS type name for registration
    - _extensibility: The extensibility kind (FINAL, APPENDABLE, MUTABLE)
    - _has_key: Whether the type has key fields
    - _serialize_cdr(): Serialize to CDR bytes
    - _deserialize_cdr(): Class method to deserialize from CDR bytes

    Example (IDL-generated):
        @dataclass
        class HelloWorld:
            index: int = 0
            message: str = ""

            _dds_type_name: ClassVar[str] = "HelloWorld"
            _extensibility: ClassVar[Extensibility] = Extensibility.APPENDABLE
            _has_key: ClassVar[bool] = False

            def _serialize_cdr(self) -> bytes: ...

            @classmethod
            def _deserialize_cdr(cls, data: bytes) -> "HelloWorld": ...

            def _serialize_key(self) -> bytes: ...
    """

    _dds_type_name: ClassVar[str]
    _extensibility: ClassVar[Extensibility]
    _has_key: ClassVar[bool]

    def _serialize_cdr(self) -> bytes | memoryview:
        """Serialize this instance to CDR bytes with encapsulation header."""
        ...

    @classmethod
    def _deserialize_cdr(cls: type[T], data: bytes | bytearray | memoryview) -> T:
        """Deserialize from CDR bytes."""
        ...

    def _serialize_key(self) -> bytes:
        """Serialize key fields only (big-endian, no encapsulation header)."""
        ...
