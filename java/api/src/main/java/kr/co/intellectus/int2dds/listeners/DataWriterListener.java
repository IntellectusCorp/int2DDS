package kr.co.intellectus.int2dds.listeners;

import kr.co.intellectus.int2dds.core.DataWriter;
import kr.co.intellectus.int2dds.status.LivelinessLostStatus;
import kr.co.intellectus.int2dds.status.OfferedDeadlineMissedStatus;
import kr.co.intellectus.int2dds.status.OfferedIncompatibleQosStatus;
import kr.co.intellectus.int2dds.status.PublicationMatchedStatus;

/**
 * Receives status-change notifications for a {@link DataWriter}.
 *
 * <p>Callbacks fire on native DDS background threads, not the caller's thread — implementations
 * must be thread-safe. Any exception thrown from a callback is described and cleared at the native
 * boundary and never propagates back into the DDS thread.
 *
 * <p>Prefer extending {@link DataWriterListenerBase} so future callbacks stay source-compatible.
 */
public interface DataWriterListener {

    /**
     * Invoked when the set of matched DataReaders changes.
     *
     * @param writer the DataWriter the listener is installed on
     * @param status the matched-reader counts at the time of the change
     */
    void onPublicationMatched(DataWriter<?> writer, PublicationMatchedStatus status);

    /**
     * Invoked when the writer misses the deadline it offered for an instance.
     *
     * @param writer the DataWriter the listener is installed on
     * @param status the missed-deadline counts at the time of the event
     */
    void onOfferedDeadlineMissed(DataWriter<?> writer, OfferedDeadlineMissedStatus status);

    /**
     * Invoked when the writer matches a DataReader with an incompatible QoS.
     *
     * @param writer the DataWriter the listener is installed on
     * @param status the incompatible-QoS counts at the time of the event
     */
    void onOfferedIncompatibleQos(DataWriter<?> writer, OfferedIncompatibleQosStatus status);

    /**
     * Invoked when the writer fails to signal its liveliness within its offered period.
     *
     * @param writer the DataWriter the listener is installed on
     * @param status the liveliness-lost counts at the time of the event
     */
    void onLivelinessLost(DataWriter<?> writer, LivelinessLostStatus status);
}
