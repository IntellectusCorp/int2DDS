"""
DomainParticipant - the entry point for DDS communication.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, TypeVar

from int2dds._ffi import CData, ffi, lib
from int2dds.core.conditions import StatusCondition
from int2dds.exceptions import DdsAlreadyDeleted, check_ret

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
    _handle: CData | None = None

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
    def handle(self) -> CData:
        if self._handle is None:
            self._initialize()
        return self._handle


def _get_factory() -> _Factory:
    """Get the DomainParticipantFactory singleton."""
    return _Factory()


def _apply_participant_qos(handle: CData, qos: ParticipantQos) -> None:
    """Apply a Python :class:`ParticipantQos` onto an existing native handle.

    Property entries are additive (merged onto whatever the handle already
    holds), preserving values the caller did not override.
    """
    if qos.user_data is not None and qos.user_data.data:
        data_ptr = ffi.from_buffer(qos.user_data.data)
        check_ret(lib.int2dds_participant_qos_set_user_data(
            handle, data_ptr, len(qos.user_data.data)))

    if qos.property is not None:
        for name, value, propagate in qos.property.entries:
            check_ret(lib.int2dds_participant_qos_add_property(
                handle, name.encode(), value.encode(), propagate))
        for name, data, propagate in getattr(qos.property, "binary_entries", []):
            data_ptr = ffi.from_buffer(data) if data else ffi.NULL
            check_ret(lib.int2dds_participant_qos_add_binary_property(
                handle, name.encode(), data_ptr, len(data), propagate))


def _build_participant_qos_handle(qos: ParticipantQos) -> CData:
    """Translate a Python :class:`ParticipantQos` into a native QoS handle.

    Caller owns the returned handle and must destroy it with
    ``lib.int2dds_participant_qos_destroy``.
    """
    qos_ptr = ffi.new("Int2DdsParticipantQos **")
    check_ret(lib.int2dds_participant_qos_create_default(qos_ptr))
    handle = qos_ptr[0]
    _apply_participant_qos(handle, qos)
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
        name: Optional label kept for API compatibility (not sent to the core)
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

        participant_ptr = ffi.new("Int2DdsParticipant **")
        if qos is None:
            check_ret(lib.int2dds_create_participant(
                factory.handle, domain_id, ffi.NULL, participant_ptr))
        else:
            qos_handle = _build_participant_qos_handle(qos)
            try:
                check_ret(lib.int2dds_create_participant(
                    factory.handle, domain_id, qos_handle, participant_ptr))
            finally:
                lib.int2dds_participant_qos_destroy(qos_handle)
        self._handle = participant_ptr[0]

    @classmethod
    def _from_looked_up_handle(cls, handle, domain_id: int) -> "DomainParticipant":
        """Wrap a handle returned by the factory's lookup_participant.

        The handle aliases an existing core participant. Closing this wrapper
        deletes the underlying participant (DDS lookup semantics); the FFI box is
        freed exactly once and the factory's delete is idempotent, so tearing
        down both the original and a looked-up handle is safe.
        """
        obj = object.__new__(cls)
        obj._domain_id = domain_id
        obj._closed = False
        obj._handle = handle
        return obj

    @staticmethod
    def lookup_participant(domain_id: int) -> "DomainParticipant | None":
        """Look up an existing participant on ``domain_id``; ``None`` if absent."""
        factory = _get_factory()
        out = ffi.new("Int2DdsParticipant **")
        check_ret(lib.int2dds_domain_participant_factory_lookup_participant(
            factory.handle, domain_id, out))
        if out[0] == ffi.NULL:
            return None
        return DomainParticipant._from_looked_up_handle(out[0], domain_id)

    @staticmethod
    def get_factory_qos() -> bool:
        """Get the factory's ``autoenable_created_entities`` policy."""
        factory = _get_factory()
        out = ffi.new("bool *")
        check_ret(lib.int2dds_domain_participant_factory_get_qos(factory.handle, out))
        return out[0]

    @staticmethod
    def set_factory_qos(autoenable_created_entities: bool) -> None:
        """Set the factory's ``autoenable_created_entities`` policy."""
        factory = _get_factory()
        check_ret(lib.int2dds_domain_participant_factory_set_qos(
            factory.handle, autoenable_created_entities))

    @staticmethod
    def set_default_participant_qos(qos: "ParticipantQos | None") -> None:
        """Set the factory's default participant QoS (``None`` resets to default)."""
        factory = _get_factory()
        if qos is None:
            check_ret(lib.int2dds_domain_participant_factory_set_default_participant_qos(
                factory.handle, ffi.NULL))
            return
        qos_handle = _build_participant_qos_handle(qos)
        try:
            check_ret(lib.int2dds_domain_participant_factory_set_default_participant_qos(
                factory.handle, qos_handle))
        finally:
            lib.int2dds_participant_qos_destroy(qos_handle)

    @staticmethod
    def get_default_participant_qos() -> "ParticipantQos":
        """Get the factory's default participant QoS (properties only)."""
        from int2dds.core.qos import ParticipantQos, Property

        factory = _get_factory()
        qos_ptr = ffi.new("Int2DdsParticipantQos **")
        check_ret(lib.int2dds_domain_participant_factory_get_default_participant_qos(
            factory.handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            prop = Property()

            @ffi.callback("int32_t(const char *, const char *, void *)")
            def _collect(name, value, _user_data):
                prop.add(ffi.string(name).decode(), ffi.string(value).decode())
                return 0

            check_ret(lib.int2dds_participant_qos_get_properties_with_prefix(
                handle, b"", _collect, ffi.NULL))
            return ParticipantQos(property=prop)
        finally:
            lib.int2dds_participant_qos_destroy(handle)

    @staticmethod
    def get_resolved_default_participant_qos() -> "ParticipantQos":
        """Get the resolved default participant QoS (registered default →
        configured default profile → spec default). Properties only."""
        from int2dds.core.qos import ParticipantQos, Property

        factory = _get_factory()
        qos_ptr = ffi.new("Int2DdsParticipantQos **")
        check_ret(lib.int2dds_domain_participant_factory_get_default_participant_qos(
            factory.handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            prop = Property()

            @ffi.callback("int32_t(const char *, const char *, void *)")
            def _collect(name, value, _user_data):
                prop.add(ffi.string(name).decode(), ffi.string(value).decode())
                return 0

            check_ret(lib.int2dds_participant_qos_get_properties_with_prefix(
                handle, b"", _collect, ffi.NULL))
            return ParticipantQos(property=prop)
        finally:
            lib.int2dds_participant_qos_destroy(handle)

    @property
    def domain_id(self) -> int:
        """Get the domain ID of this participant."""
        return self._domain_id

    @property
    def handle(self) -> CData:
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

    def create_topic_dynamic(self, topic_name: str, support):
        """Create a Topic backed by a runtime :class:`DynamicTypeSupport`."""
        from int2dds.types.dynamic import create_topic_dynamic

        return create_topic_dynamic(self, topic_name, support)

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

    def create_publisher_with_profile(self, profile: str) -> Publisher:
        """Create a Publisher whose QoS comes from the named XML profile."""
        from int2dds.core.publisher import Publisher

        return Publisher(self, profile=profile)

    def create_subscriber_with_profile(self, profile: str) -> Subscriber:
        """Create a Subscriber whose QoS comes from the named XML profile."""
        from int2dds.core.subscriber import Subscriber

        return Subscriber(self, profile=profile)

    def create_topic_with_profile(
        self,
        topic_name: str,
        type_class: type[T],
        profile: str,
    ) -> Topic[T]:
        """Create a Topic whose QoS comes from the named XML profile."""
        from int2dds.core.topic import Topic

        return Topic(self, topic_name, type_class, profile=profile)

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

    def get_current_time(self) -> tuple[int, int]:
        """Get the participant's current wall-clock time as ``(sec, nanosec)``.

        ``sec`` is a Unix timestamp (seconds since the epoch).
        """
        sec_out = ffi.new("int32_t *")
        nsec_out = ffi.new("uint32_t *")
        check_ret(lib.int2dds_participant_get_current_time(self._handle, sec_out, nsec_out))
        return sec_out[0], nsec_out[0]

    def contains_entity(self, handle: bytes) -> bool:
        """Return whether an entity with the given 16-byte instance handle
        belongs to this participant."""
        if len(handle) != 16:
            raise ValueError("instance handle must be 16 bytes")
        handle_c = ffi.new("uint8_t[16]", list(handle))
        result_out = ffi.new("bool *")
        check_ret(lib.int2dds_participant_contains_entity(
            self._handle, ffi.cast("const uint8_t(*)[16]", handle_c), result_out))
        return result_out[0]

    def find_topic(self, topic_name: str, type_class, timeout_ms: int = 0):
        """Find an existing local Topic by name, blocking up to ``timeout_ms``.

        Returns a Topic bound to ``type_class``. Use a short timeout if the topic
        may not exist — a negative timeout blocks indefinitely.
        """
        from int2dds.core.topic import Topic

        type_name = getattr(type_class, "_dds_type_name", type_class.__name__)
        topic_ptr = ffi.new("Int2DdsTopic **")
        check_ret(lib.int2dds_participant_find_topic(
            self._handle,
            topic_name.encode(),
            type_name.encode(),
            timeout_ms,
            topic_ptr,
        ))
        return Topic._from_found_handle(self, topic_ptr[0], type_class, topic_name)

    def take_discovered_publications(self, timeout_ms: int = 0) -> list[dict]:
        """Snapshot the publications (remote writers) this participant discovered."""
        from int2dds.core.discovery import take_discovered_publications

        return take_discovered_publications(self, timeout_ms)

    def take_discovered_subscriptions(self, timeout_ms: int = 0) -> list[dict]:
        """Snapshot the subscriptions (remote readers) this participant discovered."""
        from int2dds.core.discovery import take_discovered_subscriptions

        return take_discovered_subscriptions(self, timeout_ms)

    def get_statuscondition(self) -> StatusCondition:
        """Get the StatusCondition associated with this participant."""
        cond_ptr = ffi.new("Int2DdsStatusCondition **")
        check_ret(lib.int2dds_participant_get_statuscondition(self._handle, cond_ptr))
        return StatusCondition(cond_ptr[0], owner=self)

    def get_status_changes(self) -> int:
        """Get the current status change bitmask of this participant."""
        mask_out = ffi.new("uint32_t *")
        check_ret(lib.int2dds_participant_get_status_changes(self._handle, mask_out))
        return mask_out[0]

    def get_qos(self) -> ParticipantQos:
        """Get the current QoS of this participant.

        Reports the PropertyQosPolicy entries currently in effect (including any
        transport/discovery properties resolved at creation). user_data has no
        native getter and is left unset.
        """
        from int2dds.core.qos import ParticipantQos, Property

        qos_ptr = ffi.new("Int2DdsParticipantQos **")
        check_ret(lib.int2dds_participant_get_qos(self._handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            prop = Property()

            @ffi.callback("int32_t(const char *, const char *, void *)")
            def _collect(name, value, _user_data):
                prop.add(ffi.string(name).decode(), ffi.string(value).decode())
                return 0

            check_ret(lib.int2dds_participant_qos_get_properties_with_prefix(
                handle, b"", _collect, ffi.NULL))
            return ParticipantQos(property=prop)
        finally:
            lib.int2dds_participant_qos_destroy(handle)

    def set_qos(self, qos: ParticipantQos) -> None:
        """Set the QoS of this participant.

        The given policies are merged onto the participant's current QoS, so
        properties not present in ``qos`` are preserved. Setting a non-default
        user_data raises, matching the core (UserDataQosPolicy is not mutable).
        """
        qos_ptr = ffi.new("Int2DdsParticipantQos **")
        check_ret(lib.int2dds_participant_get_qos(self._handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            _apply_participant_qos(handle, qos)
            check_ret(lib.int2dds_participant_set_qos(self._handle, handle))
        finally:
            lib.int2dds_participant_qos_destroy(handle)

    def delete_contained_entities(self) -> None:
        """
        Delete all entities (Publishers, Subscribers, Topics) created by this participant.
        """
        check_ret(lib.int2dds_participant_delete_contained_entities(self._handle))

    def close(self) -> None:
        """
        Close and delete the participant.

        This will delete all contained entities first. Tolerates the participant
        already having been torn down by another handle aliasing the same core
        participant (e.g. one obtained via :meth:`lookup_participant`).
        """
        if not self._closed and self._handle is not None:
            try:
                self.delete_contained_entities()
                # The native delete frees this handle's box even when the core
                # participant was already torn down by an aliasing handle.
                check_ret(lib.int2dds_delete_participant(self._handle))
            except DdsAlreadyDeleted:
                pass  # Already torn down via an aliasing handle; box still freed.
            finally:
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
