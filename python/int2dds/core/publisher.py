"""
Publisher and DataWriter for publishing DDS data.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Generic, TypeVar

from int2dds._ffi import ffi, lib
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.participant import DomainParticipant
    from int2dds.core.qos import DataWriterQos
    from int2dds.core.topic import Topic
    from int2dds.types.base import DdsType

T = TypeVar("T", bound="DdsType")


class Publisher:
    """
    Publisher - groups DataWriters for coherent publication.

    Publishers are created through DomainParticipant.create_publisher().
    """

    __slots__ = ("_handle", "_participant", "_closed")

    def __init__(self, participant: DomainParticipant) -> None:
        self._participant = participant
        self._closed = False

        publisher_ptr = ffi.new("Int2DdsPublisher **")
        check_ret(lib.int2dds_create_publisher(participant._handle, publisher_ptr))
        self._handle = publisher_ptr[0]

    def create_datawriter(
        self,
        topic: Topic[T],
        qos: DataWriterQos | None = None,
    ) -> DataWriter[T]:
        """
        Create a DataWriter for the given topic.

        Args:
            topic: The topic to write to
            qos: Optional QoS settings

        Returns:
            A new DataWriter instance
        """
        return DataWriter(self, topic, qos)

    def delete_contained_entities(self) -> None:
        """Delete all DataWriters created by this publisher."""
        check_ret(lib.int2dds_publisher_delete_contained_entities(self._handle))

    def close(self) -> None:
        """Delete the publisher."""
        if not self._closed and self._handle is not None:
            self.delete_contained_entities()
            check_ret(lib.int2dds_delete_publisher(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> Publisher:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


class DataWriter(Generic[T]):
    """
    DataWriter - publishes data samples to a topic.

    DataWriters are created through Publisher.create_datawriter().

    Example:
        >>> writer = publisher.create_datawriter(topic)
        >>> writer.write(MyType(value=42))
    """

    __slots__ = ("_handle", "_publisher", "_topic", "_qos_handle", "_closed")

    def __init__(
        self,
        publisher: Publisher,
        topic: Topic[T],
        qos: DataWriterQos | None = None,
    ) -> None:
        self._publisher = publisher
        self._topic = topic
        self._closed = False
        self._qos_handle: ffi.CData | None = None

        # Create QoS if provided
        qos_ptr = ffi.NULL
        if qos is not None:
            qos_handle_ptr = ffi.new("Int2DdsDataWriterQos **")
            check_ret(lib.int2dds_datawriter_qos_create_default(qos_handle_ptr))
            self._qos_handle = qos_handle_ptr[0]

            # Apply QoS settings
            check_ret(
                lib.int2dds_datawriter_qos_set_reliability(
                    self._qos_handle,
                    qos.reliability._kind_int,
                    qos.reliability._max_blocking_time_ns,
                )
            )
            check_ret(
                lib.int2dds_datawriter_qos_set_durability(
                    self._qos_handle, qos.durability._kind_int
                )
            )
            check_ret(
                lib.int2dds_datawriter_qos_set_history(
                    self._qos_handle, qos.history._kind_int, qos.history.depth
                )
            )
            qos_ptr = self._qos_handle

        writer_ptr = ffi.new("Int2DdsDataWriter **")
        check_ret(
            lib.int2dds_create_datawriter(publisher._handle, topic._handle, qos_ptr, writer_ptr)
        )
        self._handle = writer_ptr[0]

        # Clean up QoS handle after use
        if self._qos_handle is not None:
            lib.int2dds_datawriter_qos_destroy(self._qos_handle)
            self._qos_handle = None

    @property
    def topic(self) -> Topic[T]:
        """Get the topic this writer publishes to."""
        return self._topic

    def write(self, sample: T) -> None:
        """
        Write a data sample.

        The sample is serialized using its _serialize_cdr() method
        and published to the topic.

        Args:
            sample: The data sample to write
        """
        # Serialize the sample
        data = sample._serialize_cdr()

        # Serialize key if the type has key fields
        key: bytes | None = None
        if getattr(sample, "_has_key", False):
            key = sample._serialize_key()

        # Write to the FFI
        data_ptr = ffi.from_buffer(data)
        key_ptr = ffi.from_buffer(key) if key else ffi.NULL
        key_len = len(key) if key else 0

        check_ret(
            lib.int2dds_write_serialized(self._handle, data_ptr, len(data), key_ptr, key_len)
        )

    def get_publication_matched_status(self) -> tuple[int, int]:
        """
        Get the publication matched status.

        Returns:
            Tuple of (total_count, current_count) indicating matched readers
        """
        total_out = ffi.new("int32_t *")
        current_out = ffi.new("int32_t *")
        check_ret(lib.int2dds_get_publication_matched_status(self._handle, total_out, current_out))
        return total_out[0], current_out[0]

    @property
    def matched_readers(self) -> int:
        """Get the current number of matched readers."""
        _, current = self.get_publication_matched_status()
        return current

    def close(self) -> None:
        """Delete the DataWriter."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_delete_datawriter(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> DataWriter[T]:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup
