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

    /** The raw native handle. Throws once this entity is closed. */
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
     * Closes every live child, then this entity.
     *
     * <p>Children go first because the native layer refuses to delete a parent
     * that still has them. Nothing calls
     * {@code int2dds_participant_delete_contained_entities}: it would free
     * handles this side still owns, and the reaper would later release them a
     * second time.
     *
     * <p>Idempotent. Throws the mapped {@code DdsException} if the native
     * delete reports a failure, which is unchecked, so callers are not forced
     * to handle it in try-with-resources.
     */
    @Override
    public final void close() {
        List<NativeEntity> live;
        synchronized (childLock) {
            live = new ArrayList<NativeEntity>(children.size());
            for (WeakReference<NativeEntity> ref : children) {
                NativeEntity child = ref.get();
                if (child != null) {
                    live.add(child);
                }
            }
            children.clear();
            sinceSweep = 0;
        }
        for (NativeEntity child : live) {
            child.close();
        }
        int rc = handle.close();
        ReturnCodes.check(rc);
    }
}
