package com.intellectus.int2dds.listeners;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.status.SubscriptionMatchedStatus;

/**
 * Receives status-change notifications for a {@link DataReader}.
 *
 * <p>Callbacks fire on native DDS background threads, not the caller's thread —
 * implementations must be thread-safe. Any exception thrown from a callback is
 * described and cleared at the native boundary and never propagates back into
 * the DDS thread.
 *
 * <p><b>v1 limitation:</b> the {@code reader} argument is always {@code null}.
 * Mapping a native handle back to its Java {@link DataReader} entity is out of
 * scope for this branch; most listeners need only the status. Prefer extending
 * {@link DataReaderListenerBase} so future callbacks stay source-compatible.
 */
public interface DataReaderListener {

    /**
     * Invoked when the set of matched DataWriters changes.
     *
     * @param reader always {@code null} in v1 (see the interface note)
     * @param status the matched-writer counts at the time of the change
     */
    void onSubscriptionMatched(DataReader<?> reader, SubscriptionMatchedStatus status);
}
