"""
Topic - associates a name with a data type.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Generic, TypeVar

from int2dds._ffi import ffi, lib
from int2dds.cdr.writer import Extensibility
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.participant import DomainParticipant
    from int2dds.core.qos import TopicQos
    from int2dds.types.base import DdsType

T = TypeVar("T", bound="DdsType")


class Topic(Generic[T]):
    """
    Topic - associates a name with a data type for publish/subscribe.

    Topics are created through DomainParticipant.create_topic().

    Attributes:
        name: The topic name
        type_name: The DDS type name
        type_class: The Python type class for serialization
    """

    __slots__ = ("_handle", "_participant", "_name", "_type_name", "_type_class", "_closed")

    def __init__(
        self,
        participant: DomainParticipant,
        topic_name: str,
        type_class: type[T],
        qos: TopicQos | None = None,
    ) -> None:
        self._participant = participant
        self._name = topic_name
        self._type_class = type_class
        self._closed = False

        # Get type metadata from the type class
        self._type_name: str = getattr(type_class, "_dds_type_name", type_class.__name__)
        extensibility: Extensibility = getattr(
            type_class, "_extensibility", Extensibility.FINAL
        )
        has_key: bool = getattr(type_class, "_has_key", False)

        topic_name_c = ffi.new("char[]", topic_name.encode())
        type_name_c = ffi.new("char[]", self._type_name.encode())

        # Create Topic QoS if provided
        qos_ptr = ffi.NULL
        qos_handle = None
        if qos is not None:
            qos_handle_ptr = ffi.new("Int2DdsTopicQos **")
            check_ret(lib.int2dds_topic_qos_create_default(qos_handle_ptr))
            qos_handle = qos_handle_ptr[0]
            if qos.reliability is not None:
                check_ret(lib.int2dds_topic_qos_set_reliability(
                    qos_handle, qos.reliability._kind_int, qos.reliability._max_blocking_time_ns))
            if qos.durability is not None:
                check_ret(lib.int2dds_topic_qos_set_durability(
                    qos_handle, qos.durability._kind_int))
            if qos.history is not None:
                check_ret(lib.int2dds_topic_qos_set_history(
                    qos_handle, qos.history._kind_int, qos.history.depth))
            if qos.deadline is not None:
                check_ret(lib.int2dds_topic_qos_set_deadline(
                    qos_handle, qos.deadline._period_ns))
            if qos.liveliness is not None:
                check_ret(lib.int2dds_topic_qos_set_liveliness(
                    qos_handle, qos.liveliness._kind_int, qos.liveliness._lease_duration_ns))
            if qos.destination_order is not None:
                check_ret(lib.int2dds_topic_qos_set_destination_order(
                    qos_handle, qos.destination_order._kind_int))
            if qos.resource_limits is not None:
                check_ret(lib.int2dds_topic_qos_set_resource_limits(
                    qos_handle,
                    qos.resource_limits.max_samples,
                    qos.resource_limits.max_instances,
                    qos.resource_limits.max_samples_per_instance))
            if qos.transport_priority is not None:
                check_ret(lib.int2dds_topic_qos_set_transport_priority(
                    qos_handle, qos.transport_priority.value))
            if qos.lifespan is not None:
                check_ret(lib.int2dds_topic_qos_set_lifespan(
                    qos_handle, qos.lifespan._duration_ns))
            if qos.ownership is not None:
                check_ret(lib.int2dds_topic_qos_set_ownership(
                    qos_handle, qos.ownership._kind_int))
            if qos.data_representation is not None:
                check_ret(lib.int2dds_topic_qos_set_data_representation(
                    qos_handle, qos.data_representation._kind_int))
            qos_ptr = qos_handle

        topic_ptr = ffi.new("Int2DdsTopic **")
        check_ret(
            lib.int2dds_create_topic_keyed(
                participant._handle,
                topic_name_c,
                type_name_c,
                int(extensibility),
                has_key,
                qos_ptr,
                topic_ptr,
            )
        )
        self._handle = topic_ptr[0]

        # Clean up QoS handle after use
        if qos_handle is not None:
            lib.int2dds_topic_qos_destroy(qos_handle)

    @property
    def name(self) -> str:
        """Get the topic name."""
        return self._name

    @property
    def type_name(self) -> str:
        """Get the DDS type name."""
        return self._type_name

    @property
    def type_class(self) -> type[T]:
        """Get the Python type class."""
        return self._type_class

    def close(self) -> None:
        """Delete the topic."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_delete_topic(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> Topic[T]:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


class ContentFilteredTopic(Generic[T]):
    """
    ContentFilteredTopic - filters data based on a SQL-like expression.

    Created through DomainParticipant.create_contentfilteredtopic().

    Attributes:
        name: The filtered topic name
        type_class: The Python type class for deserialization
        filter_expression: The SQL-92 filter expression
    """

    __slots__ = (
        "_handle", "_participant", "_related_topic", "_name",
        "_type_class", "_filter_expression", "_closed",
    )

    def __init__(
        self,
        participant: DomainParticipant,
        topic_name: str,
        related_topic: Topic[T],
        filter_expression: str,
        expression_parameters: list[str] | None = None,
    ) -> None:
        self._participant = participant
        self._related_topic = related_topic
        self._name = topic_name
        self._type_class = related_topic.type_class
        self._filter_expression = filter_expression
        self._closed = False

        if expression_parameters is None:
            expression_parameters = []

        topic_name_c = ffi.new("char[]", topic_name.encode())
        filter_expr_c = ffi.new("char[]", filter_expression.encode())

        # Build C string array for parameters
        param_ptrs = []
        param_bufs = []
        for p in expression_parameters:
            buf = ffi.new("char[]", p.encode())
            param_bufs.append(buf)
            param_ptrs.append(buf)

        if param_ptrs:
            params_arr = ffi.new("char *[]", param_ptrs)
        else:
            params_arr = ffi.NULL

        cft_ptr = ffi.new("Int2DdsContentFilteredTopic **")
        check_ret(
            lib.int2dds_create_contentfilteredtopic(
                participant._handle,
                topic_name_c,
                related_topic._handle,
                filter_expr_c,
                params_arr,
                len(expression_parameters),
                cft_ptr,
            )
        )
        self._handle = cft_ptr[0]

    @property
    def name(self) -> str:
        return self._name

    @property
    def type_class(self) -> type[T]:
        return self._type_class

    def close(self) -> None:
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_delete_contentfilteredtopic(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> ContentFilteredTopic[T]:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass
