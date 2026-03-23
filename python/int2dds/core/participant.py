"""
DomainParticipant - the entry point for DDS communication.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, TypeVar

from int2dds._ffi import ffi, lib
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.publisher import Publisher
    from int2dds.core.subscriber import Subscriber
    from int2dds.core.topic import Topic
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
    """

    __slots__ = ("_handle", "_domain_id", "_closed")

    def __init__(self, domain_id: int = 0, name: str | None = None) -> None:
        self._domain_id = domain_id
        self._closed = False

        factory = _get_factory()
        name_c = ffi.new("char[]", name.encode()) if name else ffi.NULL

        participant_ptr = ffi.new("Int2DdsParticipant **")
        check_ret(lib.int2dds_create_participant(factory.handle, name_c, domain_id, participant_ptr))
        self._handle = participant_ptr[0]

    @property
    def domain_id(self) -> int:
        """Get the domain ID of this participant."""
        return self._domain_id

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
