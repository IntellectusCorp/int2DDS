package kr.co.intellectus.int2dds.listeners;

import kr.co.intellectus.int2dds.core.DataReader;
import kr.co.intellectus.int2dds.status.LivelinessChangedStatus;
import kr.co.intellectus.int2dds.status.RequestedDeadlineMissedStatus;
import kr.co.intellectus.int2dds.status.RequestedIncompatibleQosStatus;
import kr.co.intellectus.int2dds.status.SampleLostStatus;
import kr.co.intellectus.int2dds.status.SampleRejectedStatus;
import kr.co.intellectus.int2dds.status.SubscriptionMatchedStatus;

/**
 * Receives status-change notifications for a {@link DataReader}.
 *
 * <p>Callbacks fire on native DDS background threads, not the caller's thread —
 * implementations must be thread-safe. Any exception thrown from a callback is
 * described and cleared at the native boundary and never propagates back into
 * the DDS thread.
 *
 * <p>Prefer extending {@link DataReaderListenerBase} so future callbacks stay
 * source-compatible.
 */
public interface DataReaderListener {

    /**
     * Invoked when the set of matched DataWriters changes.
     *
     * @param reader the DataReader the listener is installed on
     * @param status the matched-writer counts at the time of the change
     */
    void onSubscriptionMatched(DataReader<?> reader, SubscriptionMatchedStatus status);

    /**
     * Invoked when new data is available to read or take.
     *
     * @param reader the DataReader the listener is installed on
     */
    void onDataAvailable(DataReader<?> reader);

    /**
     * Invoked when a sample is rejected (e.g. a resource limit was exceeded).
     *
     * @param reader the DataReader the listener is installed on
     * @param status the rejection counts and reason at the time of the event
     */
    void onSampleRejected(DataReader<?> reader, SampleRejectedStatus status);

    /**
     * Invoked when the liveliness of one or more matched DataWriters changes.
     *
     * @param reader the DataReader the listener is installed on
     * @param status the alive/not-alive counts at the time of the change
     */
    void onLivelinessChanged(DataReader<?> reader, LivelinessChangedStatus status);

    /**
     * Invoked when the reader misses the deadline it requested for an instance.
     *
     * @param reader the DataReader the listener is installed on
     * @param status the missed-deadline counts at the time of the event
     */
    void onRequestedDeadlineMissed(DataReader<?> reader, RequestedDeadlineMissedStatus status);

    /**
     * Invoked when the reader matches a DataWriter with an incompatible QoS.
     *
     * @param reader the DataReader the listener is installed on
     * @param status the incompatible-QoS counts at the time of the event
     */
    void onRequestedIncompatibleQos(DataReader<?> reader, RequestedIncompatibleQosStatus status);

    /**
     * Invoked when a sample is lost before it could be delivered to this reader.
     *
     * @param reader the DataReader the listener is installed on
     * @param status the lost-sample counts at the time of the event
     */
    void onSampleLost(DataReader<?> reader, SampleLostStatus status);
}
