package com.intellectus.int2dds.listeners;

import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.status.LivelinessLostStatus;
import com.intellectus.int2dds.status.OfferedDeadlineMissedStatus;
import com.intellectus.int2dds.status.OfferedIncompatibleQosStatus;
import com.intellectus.int2dds.status.PublicationMatchedStatus;

/**
 * Receives status-change notifications for a {@link DataWriter}.
 *
 * <p>Callbacks fire on native DDS background threads, not the caller's thread —
 * implementations must be thread-safe. Any exception thrown from a callback is
 * described and cleared at the native boundary and never propagates back into
 * the DDS thread.
 *
 * <p><b>v1 limitation:</b> the {@code writer} argument is always {@code null}.
 * Mapping a native handle back to its Java {@link DataWriter} entity is out of
 * scope for this branch; most listeners need only the status. Prefer extending
 * {@link DataWriterListenerBase} so future callbacks stay source-compatible.
 */
public interface DataWriterListener {

    /**
     * Invoked when the set of matched DataReaders changes.
     *
     * @param writer always {@code null} in v1 (see the interface note)
     * @param status the matched-reader counts at the time of the change
     */
    void onPublicationMatched(DataWriter<?> writer, PublicationMatchedStatus status);

    /**
     * Invoked when the writer misses the deadline it offered for an instance.
     *
     * @param writer always {@code null} in v1 (see the interface note)
     * @param status the missed-deadline counts at the time of the event
     */
    void onOfferedDeadlineMissed(DataWriter<?> writer, OfferedDeadlineMissedStatus status);

    /**
     * Invoked when the writer matches a DataReader with an incompatible QoS.
     *
     * @param writer always {@code null} in v1 (see the interface note)
     * @param status the incompatible-QoS counts at the time of the event
     */
    void onOfferedIncompatibleQos(DataWriter<?> writer, OfferedIncompatibleQosStatus status);

    /**
     * Invoked when the writer fails to signal its liveliness within its offered period.
     *
     * @param writer always {@code null} in v1 (see the interface note)
     * @param status the liveliness-lost counts at the time of the event
     */
    void onLivelinessLost(DataWriter<?> writer, LivelinessLostStatus status);
}
