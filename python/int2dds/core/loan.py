"""Samples loaned from a DataReader's cache (DDS v1.4 2.2.2.5.3.20)."""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING, Any

from int2dds._ffi import CData, ffi, lib
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.subscriber import DataReader


class LoanedSample:
    """One loaned element: SampleInfo fields and a read-only view of the serialized bytes."""

    __slots__ = ("_owner", "_valid_data", "_instance_handle", "_instance_state", "_view")

    def __init__(self, owner: LoanedSamples, valid_data: bool, instance_handle: bytes,
                 instance_state: int, view: memoryview | None) -> None:
        self._owner = owner
        self._valid_data = valid_data
        self._instance_handle = instance_handle
        self._instance_state = instance_state
        self._view = view

    @property
    def valid_data(self) -> bool:
        self._owner._check()
        return self._valid_data

    @property
    def instance_handle(self) -> bytes:
        self._owner._check()
        return self._instance_handle

    @property
    def instance_state(self) -> int:
        self._owner._check()
        return self._instance_state

    @property
    def serialized_data(self) -> memoryview | None:
        """CDR bytes in native memory, or None without data. Released when the loan returns;
        slices taken from it, and views with a buffer still exported (e.g. PickleBuffer), are
        not, and must not outlive the loan."""
        self._owner._check()
        return self._view


class LoanedSamples(Sequence[LoanedSample]):
    """Samples loaned by a read/take; ``close()`` (or the ``with`` block) returns the loan."""

    def __init__(self, reader: DataReader[Any], handle: CData) -> None:
        self._reader = reader
        self._handle = handle
        self._returned = False
        self._views: list[memoryview] = []
        self._samples: list[LoanedSample] = []
        if handle == ffi.NULL:
            return
        try:
            info = ffi.new("Int2DdsSampleInfo *")
            data_out = ffi.new("const uint8_t **")
            size_out = ffi.new("size_t *")
            for i in range(lib.int2dds_loaned_samples_length(handle)):
                check_ret(lib.int2dds_loaned_samples_get_info(handle, i, info))
                view = None
                if info.valid_data:
                    check_ret(lib.int2dds_loaned_samples_get_data(handle, i, data_out, size_out))
                    base = memoryview(ffi.buffer(data_out[0], size_out[0]))
                    view = base.toreadonly()
                    self._views += [view, base]
                self._samples.append(LoanedSample(
                    self, bool(info.valid_data), bytes(ffi.buffer(info.instance_handle, 16)),
                    info.instance_state, view))
        except BaseException:
            lib.int2dds_datareader_return_loan(reader._handle, handle)
            self._mark_returned()
            raise

    def _check(self) -> None:
        if self._returned:
            raise ValueError("loan has been returned")

    def _mark_returned(self) -> None:
        # The native loan is already gone: record that before anything here can raise.
        self._handle = ffi.NULL
        self._returned = True
        for view in self._views:
            try:
                view.release()
            except BufferError:
                pass
        self._views.clear()

    def __len__(self) -> int:
        self._check()
        return len(self._samples)

    def __getitem__(self, index):  # type: ignore[override]
        self._check()
        return self._samples[index]

    def close(self) -> None:
        """Return the loan to its reader. Idempotent."""
        self._reader.return_loan(self)

    def __enter__(self) -> LoanedSamples:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()
