"""
WaitSet and Condition classes for event-driven programming.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from int2dds._ffi import CData, ffi, lib
from int2dds.exceptions import INT2DDS_RET_TIMEOUT, DdsTimeout, check_ret

if TYPE_CHECKING:
    from int2dds.core.publisher import DataWriter
    from int2dds.core.subscriber import DataReader


# Status mask constants (matching int2dds Rust library StatusKind bit positions)
STATUS_INCONSISTENT_TOPIC = 1 << 0
STATUS_OFFERED_DEADLINE_MISSED = 1 << 1
STATUS_REQUESTED_DEADLINE_MISSED = 1 << 2
STATUS_OFFERED_INCOMPATIBLE_QOS = 1 << 5
STATUS_REQUESTED_INCOMPATIBLE_QOS = 1 << 6
STATUS_SAMPLE_LOST = 1 << 7
STATUS_SAMPLE_REJECTED = 1 << 8
STATUS_DATA_ON_READERS = 1 << 9
STATUS_DATA_AVAILABLE = 1 << 10
STATUS_LIVELINESS_LOST = 1 << 11
STATUS_LIVELINESS_CHANGED = 1 << 12
STATUS_PUBLICATION_MATCHED = 1 << 13
STATUS_SUBSCRIPTION_MATCHED = 1 << 14


class GuardCondition:
    """
    A user-controlled condition for signaling.

    GuardConditions can be attached to a WaitSet and manually triggered
    to wake up waiting threads.

    Example:
        >>> guard = GuardCondition()
        >>> waitset.attach(guard)
        >>> guard.trigger()  # Wake up the waitset
    """

    __slots__ = ("_handle", "_closed")

    def __init__(self) -> None:
        self._closed = False
        condition_ptr = ffi.new("Int2DdsGuardCondition **")
        check_ret(lib.int2dds_guardcondition_new(condition_ptr))
        self._handle = condition_ptr[0]

    @property
    def trigger_value(self) -> bool:
        """Get the current trigger value."""
        value_out = ffi.new("bool *")
        check_ret(lib.int2dds_guardcondition_get_trigger_value(self._handle, value_out))
        return value_out[0]

    def trigger(self) -> None:
        """Set the trigger value to True."""
        check_ret(lib.int2dds_guardcondition_set_trigger_value(self._handle, True))

    def reset(self) -> None:
        """Set the trigger value to False."""
        check_ret(lib.int2dds_guardcondition_set_trigger_value(self._handle, False))

    def close(self) -> None:
        """Delete the GuardCondition."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_guardcondition_delete(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> GuardCondition:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


class StatusCondition:
    """
    A condition based on entity status changes.

    StatusConditions are obtained from DataReaders or DataWriters
    and can be attached to a WaitSet.

    Example:
        >>> condition = reader.get_status_condition()
        >>> condition.set_enabled_statuses(STATUS_DATA_AVAILABLE)
        >>> waitset.attach(condition)
    """

    __slots__ = ("_handle", "_closed", "_owner")

    def __init__(self, handle: CData, owner: object) -> None:
        self._handle = handle
        self._owner = owner  # Keep reference to prevent premature deletion
        self._closed = False

    @property
    def enabled_statuses(self) -> int:
        """Get the enabled status mask."""
        mask_out = ffi.new("uint32_t *")
        check_ret(lib.int2dds_statuscondition_get_enabled_statuses(self._handle, mask_out))
        return mask_out[0]

    @enabled_statuses.setter
    def enabled_statuses(self, mask: int) -> None:
        """Set the enabled status mask."""
        check_ret(lib.int2dds_statuscondition_set_enabled_statuses(self._handle, mask))

    def set_enabled_statuses(self, mask: int) -> None:
        """Set the enabled status mask."""
        self.enabled_statuses = mask

    @property
    def trigger_value(self) -> bool:
        """Get the current trigger value."""
        value_out = ffi.new("bool *")
        check_ret(lib.int2dds_statuscondition_get_trigger_value(self._handle, value_out))
        return value_out[0]

    def close(self) -> None:
        """Delete the StatusCondition."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_statuscondition_delete(self._handle))
            self._handle = None
            self._closed = True

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


# Sample / view / instance state mask constants (matching int2dds Rust bitflags)
READ_SAMPLE_STATE = 0x0001
NOT_READ_SAMPLE_STATE = 0x0002
ANY_SAMPLE_STATE = 0xFFFF
NEW_VIEW_STATE = 0x0001
NOT_NEW_VIEW_STATE = 0x0002
ANY_VIEW_STATE = 0xFFFF
ALIVE_INSTANCE_STATE = 0x0001
NOT_ALIVE_DISPOSED_INSTANCE_STATE = 0x0002
NOT_ALIVE_NO_WRITERS_INSTANCE_STATE = 0x0004
ANY_INSTANCE_STATE = 0xFFFF


class ReadCondition:
    """
    A condition that triggers on samples matching sample/view/instance state
    masks. Create it from a DataReader and attach it to a WaitSet.

    Example:
        >>> cond = reader.create_read_condition(
        ...     sample_states=NOT_READ_SAMPLE_STATE)
        >>> waitset.attach(cond)
        >>> waitset.wait(timeout=5.0)
        >>> samples = reader.take_w_condition(cond)
    """

    __slots__ = ("_handle", "_closed", "_owner")

    def __init__(self, handle: CData, owner: object) -> None:
        self._handle = handle
        self._owner = owner  # Keep the reader alive while the condition exists
        self._closed = False

    @property
    def trigger_value(self) -> bool:
        """Get the current trigger value."""
        value_out = ffi.new("bool *")
        check_ret(lib.int2dds_readcondition_get_trigger_value(self._handle, value_out))
        return value_out[0]

    def close(self) -> None:
        """Delete the ReadCondition."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_readcondition_delete(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> ReadCondition:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


class QueryCondition(ReadCondition):
    """
    A ReadCondition that additionally applies a SQL-92 content filter.

    Content filtering requires the topic to carry field descriptors (the same
    requirement as ContentFilteredTopic); on a plain raw topic the read raises
    an error instead of silently ignoring the filter.

    Example:
        >>> cond = reader.create_query_condition("index > %0", ["100"])
        >>> waitset.attach(cond)
        >>> samples = reader.take_w_condition(cond)
    """

    def set_query_parameters(self, parameters: list[str]) -> None:
        """Replace the parameters bound to the query expression."""
        count = len(parameters)
        encoded = [ffi.new("char[]", p.encode("utf-8")) for p in parameters]
        arr = ffi.new("char *const[]", encoded) if count else ffi.NULL
        check_ret(
            lib.int2dds_querycondition_set_query_parameters(self._handle, arr, count)
        )


class Condition:
    """
    A generic condition returned from WaitSet.wait_ex().

    Represents a triggered condition from the WaitSet. Use trigger_value
    to check if this condition is currently triggered.

    Note: This is a read-only wrapper around Int2DdsCondition.
    It cannot be attached to a WaitSet directly.

    Example:
        >>> triggered = waitset.wait_ex(timeout=5.0)
        >>> for cond in triggered:
        ...     print(f"Triggered: {cond.trigger_value}")
    """

    __slots__ = ("_handle", "_closed")

    def __init__(self, handle: CData) -> None:
        self._handle = handle
        self._closed = False

    @property
    def trigger_value(self) -> bool:
        """Get the current trigger value of this condition."""
        value_out = ffi.new("bool *")
        check_ret(lib.int2dds_condition_get_trigger_value(self._handle, value_out))
        return value_out[0]

    def close(self) -> None:
        """Delete the Condition handle."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_condition_delete(self._handle))
            self._handle = None
            self._closed = True

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


class WaitSet:
    """
    WaitSet - wait for multiple conditions.

    A WaitSet allows blocking until one or more conditions are triggered.

    Example:
        >>> waitset = WaitSet()
        >>> waitset.attach(reader)  # Attach DataReader's status condition
        >>> waitset.wait(timeout=5.0)  # Wait up to 5 seconds
        >>> for sample in reader.take():
        ...     print(sample.data)
    """

    __slots__ = ("_handle", "_closed", "_attached")

    def __init__(self) -> None:
        self._closed = False
        self._attached: list[object] = []  # Keep references to attached objects

        waitset_ptr = ffi.new("Int2DdsWaitSet **")
        check_ret(lib.int2dds_waitset_new(waitset_ptr))
        self._handle = waitset_ptr[0]

    def attach(self, condition: GuardCondition | StatusCondition | DataReader | DataWriter) -> None:
        """
        Attach a condition to the WaitSet.

        Args:
            condition: GuardCondition, StatusCondition, DataReader, or DataWriter
        """
        # Import here to avoid circular imports
        from int2dds.core.publisher import DataWriter
        from int2dds.core.subscriber import DataReader

        if isinstance(condition, GuardCondition):
            check_ret(lib.int2dds_waitset_attach_guardcondition(self._handle, condition._handle))
        elif isinstance(condition, StatusCondition):
            check_ret(lib.int2dds_waitset_attach_statuscondition(self._handle, condition._handle))
        elif isinstance(condition, ReadCondition):
            check_ret(lib.int2dds_waitset_attach_readcondition(self._handle, condition._handle))
        elif isinstance(condition, (DataReader, DataWriter)):
            cond = condition.get_statuscondition()
            check_ret(lib.int2dds_waitset_attach_statuscondition(self._handle, cond._handle))
        else:
            raise TypeError(f"Cannot attach {type(condition).__name__} to WaitSet")

        self._attached.append(condition)

    def detach(self, condition: GuardCondition | StatusCondition | DataReader | DataWriter) -> None:
        """
        Detach a condition from the WaitSet.

        Args:
            condition: Previously attached condition
        """
        from int2dds.core.publisher import DataWriter
        from int2dds.core.subscriber import DataReader

        if isinstance(condition, GuardCondition):
            check_ret(lib.int2dds_waitset_detach_guardcondition(self._handle, condition._handle))
        elif isinstance(condition, StatusCondition):
            check_ret(lib.int2dds_waitset_detach_statuscondition(self._handle, condition._handle))
        elif isinstance(condition, ReadCondition):
            check_ret(lib.int2dds_waitset_detach_readcondition(self._handle, condition._handle))
        elif isinstance(condition, (DataReader, DataWriter)):
            cond = condition.get_statuscondition()
            check_ret(lib.int2dds_waitset_detach_statuscondition(self._handle, cond._handle))
        else:
            raise TypeError(f"Cannot detach {type(condition).__name__} from WaitSet")

        if condition in self._attached:
            self._attached.remove(condition)

    def wait(self, timeout: float | None = None) -> None:
        """
        Wait for conditions to be triggered.

        Args:
            timeout: Maximum time to wait in seconds, None for infinite

        Raises:
            DdsTimeout: If the timeout expires before any condition triggers
        """
        timeout_ms = -1 if timeout is None else int(timeout * 1000)
        seq_ptr = ffi.new("Int2DdsConditionSeq **")
        ret = lib.int2dds_waitset_wait_ex(self._handle, timeout_ms, seq_ptr)

        if ret == INT2DDS_RET_TIMEOUT:
            raise DdsTimeout()
        check_ret(ret)
        if seq_ptr[0] != ffi.NULL:
            lib.int2dds_condition_seq_delete(seq_ptr[0])

    def wait_ex(self, timeout: float | None = None) -> list[Condition]:
        """
        Wait for conditions and return the list of triggered conditions.

        Unlike wait(), this method returns which conditions were triggered,
        useful when multiple conditions are attached to the WaitSet.

        Args:
            timeout: Maximum time to wait in seconds, None for infinite

        Returns:
            List of triggered Condition objects

        Raises:
            DdsTimeout: If the timeout expires before any condition triggers

        Example:
            >>> waitset.attach(reader1_cond)
            >>> waitset.attach(reader2_cond)
            >>> triggered = waitset.wait_ex(timeout=5.0)
            >>> for cond in triggered:
            ...     print(f"Triggered: {cond.trigger_value}")
        """
        timeout_ms = -1 if timeout is None else int(timeout * 1000)
        seq_ptr = ffi.new("Int2DdsConditionSeq **")
        ret = lib.int2dds_waitset_wait_ex(self._handle, timeout_ms, seq_ptr)
        return self._consume_condition_seq(ret, seq_ptr)

    def wait_ex_ns(self, timeout_ns: int | None = None) -> list[Condition]:
        """Nanosecond-precision variant of :meth:`wait_ex`.

        Args:
            timeout_ns: Maximum time to wait in nanoseconds, None for infinite.
        """
        timeout = -1 if timeout_ns is None else int(timeout_ns)
        seq_ptr = ffi.new("Int2DdsConditionSeq **")
        ret = lib.int2dds_waitset_wait_ex_ns(self._handle, timeout, seq_ptr)
        return self._consume_condition_seq(ret, seq_ptr)

    def _consume_condition_seq(self, ret: int, seq_ptr) -> list[Condition]:
        if ret == INT2DDS_RET_TIMEOUT:
            raise DdsTimeout()
        check_ret(ret)

        seq = seq_ptr[0]
        try:
            count_out = ffi.new("size_t *")
            check_ret(lib.int2dds_condition_seq_length(seq, count_out))
            conditions: list[Condition] = []
            for i in range(count_out[0]):
                cond_ptr = ffi.new("Int2DdsCondition **")
                check_ret(lib.int2dds_condition_seq_get(seq, i, cond_ptr))
                conditions.append(Condition(cond_ptr[0]))
            return conditions
        finally:
            lib.int2dds_condition_seq_delete(seq)


    def close(self) -> None:
        """Delete the WaitSet."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_waitset_delete(self._handle))
            self._handle = None
            self._closed = True
            self._attached.clear()

    def __enter__(self) -> WaitSet:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup
