package com.intellectus.int2dds.listeners;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.status.LivelinessChangedStatus;
import com.intellectus.int2dds.status.RequestedDeadlineMissedStatus;
import com.intellectus.int2dds.status.RequestedIncompatibleQosStatus;
import com.intellectus.int2dds.status.SampleLostStatus;
import com.intellectus.int2dds.status.SampleRejectedStatus;
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

    /**
     * Invoked when new data is available to read or take.
     *
     * @param reader always {@code null} in v1 (see the interface note)
     */
    void onDataAvailable(DataReader<?> reader);

    /**
     * Invoked when a sample is rejected (e.g. a resource limit was exceeded).
     *
     * @param reader always {@code null} in v1 (see the interface note)
     * @param status the rejection counts and reason at the time of the event
     */
    void onSampleRejected(DataReader<?> reader, SampleRejectedStatus status);

    /**
     * Invoked when the liveliness of one or more matched DataWriters changes.
     *
     * @param reader always {@code null} in v1 (see the interface note)
     * @param status the alive/not-alive counts at the time of the change
     */
    void onLivelinessChanged(DataReader<?> reader, LivelinessChangedStatus status);

    /**
     * Invoked when the reader misses the deadline it requested for an instance.
     *
     * @param reader always {@code null} in v1 (see the interface note)
     * @param status the missed-deadline counts at the time of the event
     */
    void onRequestedDeadlineMissed(DataReader<?> reader, RequestedDeadlineMissedStatus status);

    /**
     * Invoked when the reader matches a DataWriter with an incompatible QoS.
     *
     * @param reader always {@code null} in v1 (see the interface note)
     * @param status the incompatible-QoS counts at the time of the event
     */
    void onRequestedIncompatibleQos(DataReader<?> reader, RequestedIncompatibleQosStatus status);

    /**
     * Invoked when a sample is lost before it could be delivered to this reader.
     *
     * @param reader always {@code null} in v1 (see the interface note)
     * @param status the lost-sample counts at the time of the event
     */
    void onSampleLost(DataReader<?> reader, SampleLostStatus status);
}
