package com.intellectus.int2dds.internal;

/**
 * One native handle, released at most once.
 *
 * <p>Held by the entity that owns the handle. The reaper holds the same
 * underlying state, so whichever of the two acts first performs the release and
 * the other becomes a no-op.
 */
public final class NativeHandle {

    private final NativeCleaner.State state;
    private final NativeCleaner.Reaper reaper;

    NativeHandle(NativeCleaner.State state, NativeCleaner.Reaper reaper) {
        this.state = state;
        this.reaper = reaper;
    }

    /** The raw handle. Throws once released. */
    public long value() {
        return state.value();
    }

    public boolean isClosed() {
        return state.isClosed();
    }

    /**
     * Releases the handle if it has not been released already.
     *
     * @return the C ABI status code from the delete, or 0 if it was already
     *         released and nothing was done
     */
    public int close() {
        if (!state.release()) {
            return 0;
        }
        // Drop the phantom reference now that it has nothing left to do, so it
        // and its state can be collected without waiting to be enqueued.
        NativeCleaner.forget(reaper);
        return state.lastCode();
    }
}
