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

    /**
     * The raw handle. Throws once released — not while merely refused; see
     * {@link #close()}.
     *
     * <p>{@code CLOSED} — the point at which this starts throwing — is set
     * only after the deleter call returns successfully, not before it is
     * made. A concurrent caller of this method during that call can still
     * observe the pointer as valid and hand it back a moment before the
     * underlying native object is actually freed. That window is inherent to
     * returning a raw {@code long} rather than a checked-out resource; it is
     * not closed by this class. Entity classes built on top of this need
     * their own synchronization if they have a genuine concurrent-use
     * requirement across threads.
     */
    public long value() {
        return state.value();
    }

    public boolean isClosed() {
        return state.isClosed();
    }

    /**
     * Releases the handle if it has not been released already.
     *
     * <p>If the native layer refuses because a child is still alive, this
     * throws rather than returning as if nothing happened or as if it
     * succeeded: the entity is still alive, so reporting success would be a
     * lie, and silently swallowing the refusal would hide that the caller
     * closed things out of order. The handle is left open and usable — call
     * {@link #close()} again after releasing the blocking child, or simply
     * drop this handle's owner and let the reaper retry it automatically.
     *
     * <p>A non-refusal failure, by contrast, returns the raw code rather than
     * throwing, unchanged from this method's original contract — only a
     * refusal names a specific, actionable cause worth an exception type of
     * its own.
     *
     * @return the C ABI status code from the delete — 0 on success, a
     *     non-zero, non-refusal code if the delete failed for some other
     *     reason — or 0 if it had already been released and nothing was done
     * @throws com.intellectus.int2dds.exceptions.DdsPreconditionNotMetException
     *     if the native layer refused because a child is still alive
     */
    public int close() {
        NativeCleaner.Result result = state.release();
        switch (result.outcome) {
            case RELEASED:
                // Drop the phantom reference now that it has nothing left to
                // do, so it and its state can be collected without waiting to
                // be enqueued.
                NativeCleaner.forget(reaper);
                return result.code;
            case REFUSED:
                // Still open: a live child blocked it. ReturnCodes maps the
                // code to the exception type that names this specifically.
                // result.code is this call's own code, not a shared field a
                // concurrent attempt could have overwritten by now.
                ReturnCodes.check(result.code);
                throw new AssertionError("ReturnCodes.check did not throw for a refusal code");
            case FAILED:
                return result.code;
            case ALREADY_HANDLED:
            default:
                return 0;
        }
    }
}
