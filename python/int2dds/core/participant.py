"""
DomainParticipant - the entry point for DDS communication.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, TypeVar

from int2dds._ffi import ffi, lib
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.publisher import Publisher
    from int2dds.core.qos import ParticipantQos
    from int2dds.core.subscriber import Subscriber
    from int2dds.core.topic import ContentFilteredTopic, Topic
    from int2dds.types.base import DdsType

T = TypeVar("T", bound="DdsType")


class _Factory:
    """Singleton wrapper for DomainParticipantFactory."""

    _instance: _Factory | None = None
    _handle: ffi.CData | None = None

    def __new__(cls) -> _Factory:
        if cls._instance is None:
            cls._instance = super().__new__(cls)
            cls._instance._initialize()
        return cls._instance

    def _initialize(self) -> None:
        factory_ptr = ffi.new("Int2DdsParticipantFactory **")
        check_ret(lib.int2dds_domain_participant_factory_get_instance(factory_ptr))
        self._handle = factory_ptr[0]

    @property
    def handle(self) -> ffi.CData:
        if self._handle is None:
            self._initialize()
        return self._handle


def _get_factory() -> _Factory:
    """Get the DomainParticipantFactory singleton."""
    return _Factory()


def _build_participant_qos_handle(qos: ParticipantQos) -> ffi.CData:
    """Translate a Python :class:`ParticipantQos` into a native QoS handle.

    Caller owns the returned handle and must destroy it with
    ``lib.int2dds_participant_qos_destroy``.
    """
    qos_ptr = ffi.new("Int2DdsParticipantQos **")
    check_ret(lib.int2dds_participant_qos_create_default(qos_ptr))
    handle = qos_ptr[0]

    if qos.user_data is not None and qos.user_data.data:
        data_ptr = ffi.from_buffer(qos.user_data.data)
        check_ret(lib.int2dds_participant_qos_set_user_data(
            handle, data_ptr, len(qos.user_data.data)))

    if qos.property is not None:
        for name, value, propagate in qos.property.entries:
            check_ret(lib.int2dds_participant_qos_add_property(
                handle, name.encode(), value.encode(), propagate))

    return handle


class DomainParticipant:
    """
    DomainParticipant - the main entry point for DDS communication.

    A DomainParticipant represents the local membership of the application
    in a DDS domain. It acts as a factory for Publisher, Subscriber, and Topic.

    Example:
        >>> with DomainParticipant(domain_id=0) as dp:
        ...     topic = dp.create_topic("HelloWorld", HelloWorldType)
        ...     pub = dp.create_publisher()
        ...     writer = pub.create_datawriter(topic)
        ...     writer.write(HelloWorldType(message="Hello!"))

    Args:
        domain_id: The DDS domain to join (default 0)
        name: Optional name for the participant
        qos: Optional ParticipantQos (e.g. to set multicast TTL via PropertyQosPolicy)
    """

    __slots__ = ("_handle", "_domain_id", "_closed")

    def __init__(
        self,
        domain_id: int = 0,
        name: str | None = None,
        qos: ParticipantQos | None = None,
    ) -> None:
        self._domain_id = domain_id
        self._closed = False

        factory = _get_factory()
        name_c = ffi.new("char[]", name.encode()) if name else ffi.NULL

        participant_ptr = ffi.new("Int2DdsParticipant **")
        if qos is None:
            check_ret(lib.int2dds_create_participant(
                factory.handle, name_c, domain_id, participant_ptr))
        else:
            qos_handle = _build_participant_qos_handle(qos)
            try:
                check_ret(lib.int2dds_create_participant_with_qos(
                    factory.handle, name_c, domain_id, qos_handle, participant_ptr))
            finally:
                lib.int2dds_participant_qos_destroy(qos_handle)
        self._handle = participant_ptr[0]

    @property
    def domain_id(self) -> int:
        """Get the domain ID of this participant."""
        return self._domain_id

    @property
    def handle(self) -> ffi.CData:
        """Native participant handle (for low-level/dynamic-type FFI calls)."""
        if self._handle is None:
            raise RuntimeError("participant is closed")
        return self._handle

    def wait_for_type_object(self, topic_name: str, timeout_ms: int = -1):
        """Discover a remote type's TypeObject. Returns (TypeObject, type_name)."""
        from int2dds.types.dynamic import wait_for_type_object

        return wait_for_type_object(self, topic_name, timeout_ms)

    def decode_sample(self, type_obj, data: bytes):
        """Decode raw CDR `data` into a DynamicData using `type_obj`."""
        from int2dds.types.dynamic import decode_sample

        return decode_sample(self, type_obj, data)

    def create_publisher(self) -> Publisher:
        """
        Create a Publisher for this participant.

        Returns:
            A new Publisher instance
        """
        from int2dds.core.publisher import Publisher

        return Publisher(self)

    def create_subscriber(self) -> Subscriber:
        """
        Create a Subscriber for this participant.

        Returns:
            A new Subscriber instance
        """
        from int2dds.core.subscriber import Subscriber

        return Subscriber(self)

    def create_topic(
        self,
        topic_name: str,
        type_class: type[T],
        qos: None = None,
    ) -> Topic[T]:
        """
        Create a Topic for this participant.

        Args:
            topic_name: The name of the topic
            type_class: The DDS type class (must have _dds_type_name and _extensibility)
            qos: Optional TopicQos (not yet implemented)

        Returns:
            A new Topic instance
        """
        from int2dds.core.topic import Topic

        return Topic(self, topic_name, type_class, qos)

    def create_contentfilteredtopic(
        self,
        topic_name: str,
        related_topic: Topic[T],
        filter_expression: str,
        expression_parameters: list[str] | None = None,
    ) -> ContentFilteredTopic[T]:
        """
        Create a ContentFilteredTopic that filters data using a SQL-92 expression.

        Args:
            topic_name: Name for the filtered topic
            related_topic: The original Topic to filter
            filter_expression: SQL-92 filter (e.g., "color = %0")
            expression_parameters: Parameter values (e.g., ["RED"])

        Returns:
            A new ContentFilteredTopic instance
        """
        from int2dds.core.topic import ContentFilteredTopic

        return ContentFilteredTopic(
            self, topic_name, related_topic, filter_expression, expression_parameters
        )

    def assert_liveliness(self) -> None:
        """
        Assert liveliness for MANUAL_BY_PARTICIPANT liveliness.
        """
        check_ret(lib.int2dds_participant_assert_liveliness(self._handle))

    def delete_contained_entities(self) -> None:
        """
        Delete all entities (Publishers, Subscribers, Topics) created by this participant.
        """
        check_ret(lib.int2dds_participant_delete_contained_entities(self._handle))

    def close(self) -> None:
        """
        Close and delete the participant.

        This will delete all contained entities first.
        """
        if not self._closed and self._handle is not None:
            self.delete_contained_entities()
            check_ret(lib.int2dds_delete_participant(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> DomainParticipant:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup
