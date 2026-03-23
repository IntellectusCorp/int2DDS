"""
Async support for int2dds - provides asyncio-compatible APIs.
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, AsyncIterator, Generic, TypeVar

from int2dds.core.conditions import WaitSet
from int2dds.exceptions import DdsTimeout

if TYPE_CHECKING:
    from int2dds.core.subscriber import DataReader, Sample
    from int2dds.types.base import DdsType

T = TypeVar("T", bound="DdsType")


class AsyncWaitSet:
    """
    Async-compatible WaitSet wrapper.

    Provides async/await interface for waiting on DDS conditions.

    Example:
        >>> async_waitset = AsyncWaitSet()
        >>> async_waitset.attach(reader)
        >>> await async_waitset.wait(timeout=5.0)
        >>> for sample in reader.take():
        ...     print(sample.data)
    """

    __slots__ = ("_waitset",)

    def __init__(self) -> None:
        self._waitset = WaitSet()

    def attach(self, condition: object) -> None:
        """Attach a condition to the WaitSet."""
        self._waitset.attach(condition)

    def detach(self, condition: object) -> None:
        """Detach a condition from the WaitSet."""
        self._waitset.detach(condition)

    async def wait(self, timeout: float | None = None) -> None:
        """
        Asynchronously wait for conditions to be triggered.

        This runs the blocking wait in a thread pool executor,
        allowing other async tasks to run concurrently.

        Args:
            timeout: Maximum time to wait in seconds, None for infinite

        Raises:
            DdsTimeout: If the timeout expires before any condition triggers
        """
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, self._waitset.wait, timeout)

    def close(self) -> None:
        """Close the underlying WaitSet."""
        self._waitset.close()

    async def __aenter__(self) -> AsyncWaitSet:
        return self

    async def __aexit__(
        self, exc_type: object, exc_val: object, exc_tb: object
    ) -> None:
        self.close()


class AsyncDataReader(Generic[T]):
    """
    Async-compatible DataReader wrapper.

    Provides async/await interface for reading DDS samples.

    Example:
        >>> async_reader = AsyncDataReader(reader)
        >>> async for sample in async_reader:
        ...     if sample.valid_data:
        ...         print(sample.data)
    """

    __slots__ = ("_reader", "_waitset", "_poll_interval")

    def __init__(self, reader: DataReader[T], poll_interval: float = 0.1) -> None:
        """
        Create an async wrapper for a DataReader.

        Args:
            reader: The underlying DataReader
            poll_interval: Interval between data checks when using async iteration
        """
        self._reader = reader
        self._waitset: AsyncWaitSet | None = None
        self._poll_interval = poll_interval

    @property
    def reader(self) -> DataReader[T]:
        """Get the underlying DataReader."""
        return self._reader

    async def take(self) -> list[Sample[T]]:
        """
        Asynchronously take all available samples.

        This is non-blocking and returns immediately with available samples.
        """
        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(None, self._reader.take)

    async def read(self) -> list[Sample[T]]:
        """
        Asynchronously read all available samples without removing them.
        """
        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(None, self._reader.read)

    async def wait_for_data(self, timeout: float | None = None) -> bool:
        """
        Wait for data to become available.

        Args:
            timeout: Maximum time to wait in seconds

        Returns:
            True if data is available, False if timeout occurred
        """
        if self._waitset is None:
            self._waitset = AsyncWaitSet()
            self._waitset.attach(self._reader)

        try:
            await self._waitset.wait(timeout=timeout)
            return True
        except DdsTimeout:
            return False

    async def take_with_wait(self, timeout: float | None = None) -> list[Sample[T]]:
        """
        Wait for data and take samples.

        Combines wait_for_data and take into a single operation.

        Args:
            timeout: Maximum time to wait in seconds

        Returns:
            List of samples (may be empty if timeout occurred)
        """
        await self.wait_for_data(timeout=timeout)
        return await self.take()

    async def __aiter__(self) -> AsyncIterator[Sample[T]]:
        """
        Async iterator over samples.

        Yields samples as they become available.
        Use Ctrl+C or break to stop iteration.

        Example:
            >>> async for sample in async_reader:
            ...     print(sample.data)
        """
        while True:
            samples = await self.take()
            for sample in samples:
                yield sample

            if not samples:
                # No samples available, wait a bit before polling again
                await asyncio.sleep(self._poll_interval)

    def close(self) -> None:
        """Close resources."""
        if self._waitset is not None:
            self._waitset.close()
            self._waitset = None


async def async_wait(
    waitset: WaitSet, timeout: float | None = None
) -> None:
    """
    Utility function to await on a regular WaitSet.

    Args:
        waitset: The WaitSet to wait on
        timeout: Maximum time to wait in seconds

    Raises:
        DdsTimeout: If the timeout expires
    """
    loop = asyncio.get_running_loop()
    await loop.run_in_executor(None, waitset.wait, timeout)
