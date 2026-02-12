"""
WaitSet and Condition classes for event-driven programming.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from int2dds._ffi import ffi, lib
from int2dds.exceptions import INT2DDS_RET_TIMEOUT, DdsTimeout, check_ret

if TYPE_CHECKING:
    from int2dds.core.publisher import DataWriter
    from int2dds.core.subscriber import DataReader


# Status mask constants
STATUS_DATA_ON_READERS = 1 << 0
STATUS_DATA_AVAILABLE = 1 << 1
STATUS_SAMPLE_REJECTED = 1 << 2
STATUS_LIVELINESS_CHANGED = 1 << 3
STATUS_REQUESTED_DEADLINE_MISSED = 1 << 4
STATUS_REQUESTED_INCOMPATIBLE_QOS = 1 << 5
STATUS_SAMPLE_LOST = 1 << 6
STATUS_SUBSCRIPTION_MATCHED = 1 << 7
STATUS_OFFERED_DEADLINE_MISSED = 1 << 8
STATUS_OFFERED_INCOMPATIBLE_QOS = 1 << 9
STATUS_LIVELINESS_LOST = 1 << 10
STATUS_PUBLICATION_MATCHED = 1 << 11


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
        check_ret(lib.int2dds_guard_condition_new(condition_ptr))
        self._handle = condition_ptr[0]

    @property
    def trigger_value(self) -> bool:
        """Get the current trigger value."""
        value_out = ffi.new("bool *")
        check_ret(lib.int2dds_guard_condition_get_trigger_value(self._handle, value_out))
        return value_out[0]

    def trigger(self) -> None:
        """Set the trigger value to True."""
        check_ret(lib.int2dds_guard_condition_set_trigger_value(self._handle, True))

    def reset(self) -> None:
        """Set the trigger value to False."""
        check_ret(lib.int2dds_guard_condition_set_trigger_value(self._handle, False))

    def close(self) -> None:
        """Delete the GuardCondition."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_guard_condition_delete(self._handle))
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

    def __init__(self, handle: ffi.CData, owner: object) -> None:
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
            check_ret(lib.int2dds_waitset_attach_guard_condition(self._handle, condition._handle))
        elif isinstance(condition, StatusCondition):
            check_ret(lib.int2dds_waitset_attach_condition(self._handle, condition._handle))
        elif isinstance(condition, DataReader):
            check_ret(lib.int2dds_waitset_attach_datareader(self._handle, condition._handle))
        elif isinstance(condition, DataWriter):
            check_ret(lib.int2dds_waitset_attach_datawriter(self._handle, condition._handle))
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
            check_ret(lib.int2dds_waitset_detach_guard_condition(self._handle, condition._handle))
        elif isinstance(condition, StatusCondition):
            check_ret(lib.int2dds_waitset_detach_condition(self._handle, condition._handle))
        elif isinstance(condition, DataReader):
            check_ret(lib.int2dds_waitset_detach_datareader(self._handle, condition._handle))
        elif isinstance(condition, DataWriter):
            check_ret(lib.int2dds_waitset_detach_datawriter(self._handle, condition._handle))
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
        ret = lib.int2dds_waitset_wait(self._handle, timeout_ms)

        if ret == INT2DDS_RET_TIMEOUT:
            raise DdsTimeout()
        check_ret(ret)

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
