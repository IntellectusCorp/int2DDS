package com.intellectus.int2dds.core;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.ReturnCodes;
import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/**
 * Common machinery for an entity that owns a native handle.
 *
 * <p>The reference directions below are deliberate and opposite, and both are
 * load bearing, but neither one sequences the two ends' <em>reaping</em>
 * relative to each other — that is not available to arrange from Java at all.
 * Reachability propagates: an entity reachable only through a
 * phantom-reachable child is itself at most phantom-reachable, so a parent
 * and an abandoned child that die together are discovered and enqueued in the
 * very same GC cycle, and nothing orders two references discovered together.
 * See {@link NativeCleaner}'s class doc for the full argument.
 *
 * <p>A child holds its parent <em>strongly</em> so that, for as long as the
 * child itself is still reachable, the parent cannot become
 * phantom-reachable either. That is its whole job: keeping a parent alive
 * while the user still holds a child of it, not ordering the reaper, which it
 * cannot do. A parent holds its children <em>weakly</em> so that an explicit
 * {@link #close()} can still walk to every live child and close it first,
 * without that registry keeping every child permanently reachable on its own
 * account.
 *
 * <p>What actually makes it safe for the reaper to reach a parent before its
 * child, on a cycle where that happens, is the native layer, not either
 * reference direction: every {@code int2dds_delete_*} restores the caller's
 * handle rather than freeing it on failure, so a parent tried first is simply
 * refused with {@code INT2DDS_RET_PRECONDITION_NOT_MET} (24 — {@code
 * ffi/src/error.rs}'s numbering, not the DDS spec's 4), stays valid, and is
 * retried with backoff by {@link NativeCleaner}'s reaper thread until the
 * blocking child is gone.
 */
abstract class NativeEntity implements AutoCloseable {

    /** Sweep dead child references after this many additions. */
    private static final int SWEEP_INTERVAL = 64;

    private final NativeHandle handle;
    private final NativeEntity parent;

    private final Object childLock = new Object();
    private final List<WeakReference<NativeEntity>> children =
            new ArrayList<WeakReference<NativeEntity>>();
    private int sinceSweep;

    NativeEntity(NativeEntity parent, long rawHandle, NativeCleaner.Deleter deleter) {
        this.parent = parent;
        this.handle = NativeCleaner.register(this, rawHandle, deleter);
        if (parent != null) {
            parent.addChild(this);
        }
    }

    /**
     * The raw native handle. Throws once this entity is closed.
     *
     * <p>A caller that passes the returned value into a native call must keep
     * this entity reachable for the duration of that call — see {@link
     * NativeKeepAlive}. Nothing about the returned value itself keeps this
     * entity, or the native object it names, alive once this method has
     * returned: if this was the entity's last reference, the reaper can
     * enqueue and release it while a native call still using the bare {@code
     * long} is in flight.
     */
    final long handle() {
        return handle.value();
    }

    /**
     * The owning entity, or null for a root.
     *
     * <p>Also the field that keeps the parent reachable; that is its main job,
     * and it would look unused without this accessor.
     */
    final NativeEntity parent() {
        return parent;
    }

    public final boolean isClosed() {
        return handle.isClosed();
    }

    private void addChild(NativeEntity child) {
        synchronized (childLock) {
            children.add(new WeakReference<NativeEntity>(child));
            if (++sinceSweep >= SWEEP_INTERVAL) {
                sweepLocked();
                sinceSweep = 0;
            }
        }
    }

    /** Drops references whose child has already been collected. */
    private void sweepLocked() {
        for (Iterator<WeakReference<NativeEntity>> it = children.iterator(); it.hasNext();) {
            if (it.next().get() == null) {
                it.remove();
            }
        }
    }

    /**
     * Removes {@code child}'s own entry from the registry. Called only after
     * {@code child} has actually, successfully closed — a child that fails to
     * close stays registered, so a later retry of {@link #close()} can still
     * reach it. At most one entry can match, since {@link #addChild} runs
     * exactly once per child, during that child's construction.
     */
    private void removeChildLocked(NativeEntity child) {
        Iterator<WeakReference<NativeEntity>> it = children.iterator();
        while (it.hasNext()) {
            if (it.next().get() == child) {
                it.remove();
                return;
            }
        }
    }

    /**
     * Closes every live child, then this entity.
     *
     * <p>Children are closed in the reverse of the order they were added, not
     * insertion order. The native layer can refuse a delete for reasons that
     * cross the Java ownership tree, not just direct parent/child
     * containment — deleting a Topic is refused while any DataWriter still
     * uses it, even though a DataWriter's Java parent is the Publisher, not
     * the Topic. The natural create order is Topic, then Publisher, then
     * (inside the Publisher) its DataWriter, so closing in reverse closes the
     * Publisher — which cascades to its own DataWriter first — before the
     * Topic, satisfying both constraints.
     *
     * <p>Each child is closed in its own {@code try}. One child failing does
     * not stop the rest: every live child gets a chance, failures are
     * collected, and the first is rethrown with the others attached via
     * {@link Throwable#addSuppressed}. A child is removed from this entity's
     * registry only once it has actually, successfully closed, so a child
     * that fails or throws stays reachable from here for a later retry. If
     * any child failed, this entity's own handle is left completely
     * untouched — still open, still retryable — rather than spent on a
     * native delete that a live child would just get refused again anyway.
     *
     * <p>Nothing calls
     * {@code int2dds_participant_delete_contained_entities}: it would free
     * handles this side still owns, and the reaper would later release them a
     * second time.
     *
     * <p>Idempotent. Throws the mapped {@code DdsException} (or, if one or
     * more children failed, the first such failure with the rest suppressed)
     * if a delete reports a failure. Unchecked, so callers are not forced to
     * handle it in try-with-resources.
     */
    @Override
    public final void close() {
        List<NativeEntity> live;
        synchronized (childLock) {
            live = new ArrayList<NativeEntity>(children.size());
            for (Iterator<WeakReference<NativeEntity>> it = children.iterator(); it.hasNext();) {
                NativeEntity child = it.next().get();
                if (child == null) {
                    // Already collected: nothing to close, nothing to retry.
                    it.remove();
                } else {
                    live.add(child);
                }
            }
            sinceSweep = 0;
        }

        RuntimeException firstFailure = null;
        for (int i = live.size() - 1; i >= 0; i--) {
            NativeEntity child = live.get(i);
            try {
                child.close();
                synchronized (childLock) {
                    removeChildLocked(child);
                }
            } catch (RuntimeException e) {
                if (firstFailure == null) {
                    firstFailure = e;
                } else {
                    firstFailure.addSuppressed(e);
                }
            }
        }
        if (firstFailure != null) {
            throw firstFailure;
        }

        int rc = handle.close();
        ReturnCodes.check(rc);
    }
}
