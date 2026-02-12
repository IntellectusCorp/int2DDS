"""
Listener classes for event-driven DDS programming.

Listeners provide callback-based notification of DDS events
such as data arrival, publication matching, etc.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Callable, Protocol, runtime_checkable
from weakref import WeakValueDictionary

from int2dds._ffi import ffi, lib

if TYPE_CHECKING:
    from int2dds.core.publisher import DataWriter
    from int2dds.core.subscriber import DataReader


# Status dataclasses
@dataclass
class PublicationMatchedStatus:
    """Status of publication matching for a DataWriter."""

    total_count: int
    """Total cumulative count of matched DataReaders."""

    total_count_change: int
    """Change in total_count since last access."""

    current_count: int
    """Current number of matched DataReaders."""

    current_count_change: int
    """Change in current_count since last access."""


@dataclass
class SubscriptionMatchedStatus:
    """Status of subscription matching for a DataReader."""

    total_count: int
    """Total cumulative count of matched DataWriters."""

    total_count_change: int
    """Change in total_count since last access."""

    current_count: int
    """Current number of matched DataWriters."""

    current_count_change: int
    """Change in current_count since last access."""


@dataclass
class OfferedDeadlineMissedStatus:
    """Status when a DataWriter misses its deadline."""

    total_count: int
    """Total cumulative count of missed deadlines."""

    total_count_change: int
    """Change in total_count since last access."""


@dataclass
class RequestedDeadlineMissedStatus:
    """Status when a DataReader misses a requested deadline."""

    total_count: int
    """Total cumulative count of missed deadlines."""

    total_count_change: int
    """Change in total_count since last access."""


@dataclass
class LivelinessLostStatus:
    """Status when a DataWriter loses liveliness."""

    total_count: int
    """Total cumulative count of times liveliness was lost."""

    total_count_change: int
    """Change in total_count since last access."""


@dataclass
class LivelinessChangedStatus:
    """Status when liveliness changes for a DataReader."""

    alive_count: int
    """Current count of alive DataWriters."""

    not_alive_count: int
    """Current count of not-alive DataWriters."""

    alive_count_change: int
    """Change in alive_count since last access."""

    not_alive_count_change: int
    """Change in not_alive_count since last access."""


@dataclass
class SampleLostStatus:
    """Status when samples are lost."""

    total_count: int
    """Total cumulative count of lost samples."""

    total_count_change: int
    """Change in total_count since last access."""


# Status mask constants
STATUS_MASK_NONE = 0
STATUS_MASK_ALL = 0xFFFFFFFF
STATUS_DATA_AVAILABLE = 1 << 1
STATUS_SUBSCRIPTION_MATCHED = 1 << 7
STATUS_PUBLICATION_MATCHED = 1 << 11
STATUS_LIVELINESS_CHANGED = 1 << 3
STATUS_LIVELINESS_LOST = 1 << 10
STATUS_OFFERED_DEADLINE_MISSED = 1 << 8
STATUS_REQUESTED_DEADLINE_MISSED = 1 << 4
STATUS_SAMPLE_LOST = 1 << 6


# Listener protocols
@runtime_checkable
class DataWriterListener(Protocol):
    """Protocol for DataWriter listeners."""

    def on_publication_matched(
        self, writer: DataWriter, status: PublicationMatchedStatus
    ) -> None:
        """Called when publication matching status changes."""
        ...

    def on_offered_deadline_missed(
        self, writer: DataWriter, status: OfferedDeadlineMissedStatus
    ) -> None:
        """Called when the writer misses its offered deadline."""
        ...

    def on_liveliness_lost(
        self, writer: DataWriter, status: LivelinessLostStatus
    ) -> None:
        """Called when the writer loses liveliness."""
        ...


@runtime_checkable
class DataReaderListener(Protocol):
    """Protocol for DataReader listeners."""

    def on_data_available(self, reader: DataReader) -> None:
        """Called when new data is available to read."""
        ...

    def on_subscription_matched(
        self, reader: DataReader, status: SubscriptionMatchedStatus
    ) -> None:
        """Called when subscription matching status changes."""
        ...

    def on_liveliness_changed(
        self, reader: DataReader, status: LivelinessChangedStatus
    ) -> None:
        """Called when liveliness of matched writers changes."""
        ...

    def on_requested_deadline_missed(
        self, reader: DataReader, status: RequestedDeadlineMissedStatus
    ) -> None:
        """Called when the requested deadline is missed."""
        ...

    def on_sample_lost(self, reader: DataReader, status: SampleLostStatus) -> None:
        """Called when samples are lost."""
        ...


# Listener base classes with default implementations
class DataWriterListenerBase:
    """
    Base class for DataWriter listeners with empty default implementations.

    Subclass this and override only the methods you need.

    Example:
        >>> class MyListener(DataWriterListenerBase):
        ...     def on_publication_matched(self, writer, status):
        ...         print(f"Matched {status.current_count} readers")
    """

    def on_publication_matched(
        self, writer: DataWriter, status: PublicationMatchedStatus
    ) -> None:
        pass

    def on_offered_deadline_missed(
        self, writer: DataWriter, status: OfferedDeadlineMissedStatus
    ) -> None:
        pass

    def on_liveliness_lost(
        self, writer: DataWriter, status: LivelinessLostStatus
    ) -> None:
        pass


class DataReaderListenerBase:
    """
    Base class for DataReader listeners with empty default implementations.

    Subclass this and override only the methods you need.

    Example:
        >>> class MyListener(DataReaderListenerBase):
        ...     def on_data_available(self, reader):
        ...         for sample in reader.take():
        ...             print(sample.data)
    """

    def on_data_available(self, reader: DataReader) -> None:
        pass

    def on_subscription_matched(
        self, reader: DataReader, status: SubscriptionMatchedStatus
    ) -> None:
        pass

    def on_liveliness_changed(
        self, reader: DataReader, status: LivelinessChangedStatus
    ) -> None:
        pass

    def on_requested_deadline_missed(
        self, reader: DataReader, status: RequestedDeadlineMissedStatus
    ) -> None:
        pass

    def on_sample_lost(self, reader: DataReader, status: SampleLostStatus) -> None:
        pass


# Internal registry for keeping callbacks alive
_writer_registry: WeakValueDictionary[int, DataWriter] = WeakValueDictionary()
_reader_registry: WeakValueDictionary[int, DataReader] = WeakValueDictionary()
_callback_handles: dict[int, tuple] = {}  # Keep cffi callbacks alive
_next_id = 1


def _get_next_id() -> int:
    global _next_id
    id_ = _next_id
    _next_id += 1
    return id_


# C callback implementations
@ffi.callback("void(Int2DdsDataWriter*, Int2DdsPublicationMatchedStatus*, void*)")
def _on_publication_matched_cb(writer_ptr, status_ptr, user_context):
    """C callback for on_publication_matched."""
    ctx_id = int(ffi.cast("uintptr_t", user_context))
    if ctx_id not in _callback_handles:
        return
    listener, writer = _callback_handles[ctx_id][:2]
    status = PublicationMatchedStatus(
        total_count=status_ptr.total_count,
        total_count_change=status_ptr.total_count_change,
        current_count=status_ptr.current_count,
        current_count_change=status_ptr.current_count_change,
    )
    try:
        listener.on_publication_matched(writer, status)
    except Exception:
        pass  # Exceptions in callbacks should not propagate to C


@ffi.callback("void(Int2DdsDataWriter*, Int2DdsOfferedDeadlineMissedStatus*, void*)")
def _on_offered_deadline_missed_cb(writer_ptr, status_ptr, user_context):
    """C callback for on_offered_deadline_missed."""
    ctx_id = int(ffi.cast("uintptr_t", user_context))
    if ctx_id not in _callback_handles:
        return
    listener, writer = _callback_handles[ctx_id][:2]
    status = OfferedDeadlineMissedStatus(
        total_count=status_ptr.total_count,
        total_count_change=status_ptr.total_count_change,
    )
    try:
        listener.on_offered_deadline_missed(writer, status)
    except Exception:
        pass


@ffi.callback("void(Int2DdsDataWriter*, Int2DdsLivelinessLostStatus*, void*)")
def _on_liveliness_lost_cb(writer_ptr, status_ptr, user_context):
    """C callback for on_liveliness_lost."""
    ctx_id = int(ffi.cast("uintptr_t", user_context))
    if ctx_id not in _callback_handles:
        return
    listener, writer = _callback_handles[ctx_id][:2]
    status = LivelinessLostStatus(
        total_count=status_ptr.total_count,
        total_count_change=status_ptr.total_count_change,
    )
    try:
        listener.on_liveliness_lost(writer, status)
    except Exception:
        pass


@ffi.callback("void(Int2DdsDataReader*, void*)")
def _on_data_available_cb(reader_ptr, user_context):
    """C callback for on_data_available."""
    ctx_id = int(ffi.cast("uintptr_t", user_context))
    if ctx_id not in _callback_handles:
        return
    listener, reader = _callback_handles[ctx_id][:2]
    try:
        listener.on_data_available(reader)
    except Exception:
        pass


@ffi.callback("void(Int2DdsDataReader*, Int2DdsSubscriptionMatchedStatus*, void*)")
def _on_subscription_matched_cb(reader_ptr, status_ptr, user_context):
    """C callback for on_subscription_matched."""
    ctx_id = int(ffi.cast("uintptr_t", user_context))
    if ctx_id not in _callback_handles:
        return
    listener, reader = _callback_handles[ctx_id][:2]
    status = SubscriptionMatchedStatus(
        total_count=status_ptr.total_count,
        total_count_change=status_ptr.total_count_change,
        current_count=status_ptr.current_count,
        current_count_change=status_ptr.current_count_change,
    )
    try:
        listener.on_subscription_matched(reader, status)
    except Exception:
        pass


@ffi.callback("void(Int2DdsDataReader*, Int2DdsLivelinessChangedStatus*, void*)")
def _on_liveliness_changed_cb(reader_ptr, status_ptr, user_context):
    """C callback for on_liveliness_changed."""
    ctx_id = int(ffi.cast("uintptr_t", user_context))
    if ctx_id not in _callback_handles:
        return
    listener, reader = _callback_handles[ctx_id][:2]
    status = LivelinessChangedStatus(
        alive_count=status_ptr.alive_count,
        not_alive_count=status_ptr.not_alive_count,
        alive_count_change=status_ptr.alive_count_change,
        not_alive_count_change=status_ptr.not_alive_count_change,
    )
    try:
        listener.on_liveliness_changed(reader, status)
    except Exception:
        pass


@ffi.callback("void(Int2DdsDataReader*, Int2DdsRequestedDeadlineMissedStatus*, void*)")
def _on_requested_deadline_missed_cb(reader_ptr, status_ptr, user_context):
    """C callback for on_requested_deadline_missed."""
    ctx_id = int(ffi.cast("uintptr_t", user_context))
    if ctx_id not in _callback_handles:
        return
    listener, reader = _callback_handles[ctx_id][:2]
    status = RequestedDeadlineMissedStatus(
        total_count=status_ptr.total_count,
        total_count_change=status_ptr.total_count_change,
    )
    try:
        listener.on_requested_deadline_missed(reader, status)
    except Exception:
        pass


@ffi.callback("void(Int2DdsDataReader*, Int2DdsSampleLostStatus*, void*)")
def _on_sample_lost_cb(reader_ptr, status_ptr, user_context):
    """C callback for on_sample_lost."""
    ctx_id = int(ffi.cast("uintptr_t", user_context))
    if ctx_id not in _callback_handles:
        return
    listener, reader = _callback_handles[ctx_id][:2]
    status = SampleLostStatus(
        total_count=status_ptr.total_count,
        total_count_change=status_ptr.total_count_change,
    )
    try:
        listener.on_sample_lost(reader, status)
    except Exception:
        pass


def _create_writer_listener_struct(
    listener: DataWriterListener, writer: DataWriter
) -> tuple[ffi.CData, int]:
    """Create a C listener struct for a DataWriter."""
    ctx_id = _get_next_id()
    ctx_ptr = ffi.cast("void*", ctx_id)

    c_listener = ffi.new("Int2DdsDataWriterListener *")
    c_listener.on_publication_matched = _on_publication_matched_cb
    c_listener.on_offered_deadline_missed = _on_offered_deadline_missed_cb
    c_listener.on_liveliness_lost = _on_liveliness_lost_cb
    c_listener.on_offered_incompatible_qos = ffi.NULL
    c_listener.user_context = ctx_ptr

    # Store reference to keep callbacks alive
    _callback_handles[ctx_id] = (listener, writer, c_listener)

    return c_listener, ctx_id


def _create_reader_listener_struct(
    listener: DataReaderListener, reader: DataReader
) -> tuple[ffi.CData, int]:
    """Create a C listener struct for a DataReader."""
    ctx_id = _get_next_id()
    ctx_ptr = ffi.cast("void*", ctx_id)

    c_listener = ffi.new("Int2DdsDataReaderListener *")
    c_listener.on_data_available = _on_data_available_cb
    c_listener.on_subscription_matched = _on_subscription_matched_cb
    c_listener.on_liveliness_changed = _on_liveliness_changed_cb
    c_listener.on_requested_deadline_missed = _on_requested_deadline_missed_cb
    c_listener.on_sample_lost = _on_sample_lost_cb
    c_listener.on_sample_rejected = ffi.NULL
    c_listener.on_requested_incompatible_qos = ffi.NULL
    c_listener.user_context = ctx_ptr

    # Store reference to keep callbacks alive
    _callback_handles[ctx_id] = (listener, reader, c_listener)

    return c_listener, ctx_id


def _remove_listener(ctx_id: int) -> None:
    """Remove a listener from the registry."""
    if ctx_id in _callback_handles:
        del _callback_handles[ctx_id]
