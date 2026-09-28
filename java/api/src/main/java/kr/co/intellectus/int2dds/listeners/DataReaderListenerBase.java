package kr.co.intellectus.int2dds.listeners;

import kr.co.intellectus.int2dds.core.DataReader;
import kr.co.intellectus.int2dds.status.LivelinessChangedStatus;
import kr.co.intellectus.int2dds.status.RequestedDeadlineMissedStatus;
import kr.co.intellectus.int2dds.status.RequestedIncompatibleQosStatus;
import kr.co.intellectus.int2dds.status.SampleLostStatus;
import kr.co.intellectus.int2dds.status.SampleRejectedStatus;
import kr.co.intellectus.int2dds.status.SubscriptionMatchedStatus;

/**
 * No-op {@link DataReaderListener}. Extend this and override only the callbacks
 * you need; later callbacks added to the interface land here as no-ops, so
 * subclasses stay source-compatible.
 */
public class DataReaderListenerBase implements DataReaderListener {

    @Override
    public void onSubscriptionMatched(DataReader<?> reader, SubscriptionMatchedStatus status) {
        // no-op
    }

    @Override
    public void onDataAvailable(DataReader<?> reader) {
        // no-op
    }

    @Override
    public void onSampleRejected(DataReader<?> reader, SampleRejectedStatus status) {
        // no-op
    }

    @Override
    public void onLivelinessChanged(DataReader<?> reader, LivelinessChangedStatus status) {
        // no-op
    }

    @Override
    public void onRequestedDeadlineMissed(DataReader<?> reader, RequestedDeadlineMissedStatus status) {
        // no-op
    }

    @Override
    public void onRequestedIncompatibleQos(DataReader<?> reader, RequestedIncompatibleQosStatus status) {
        // no-op
    }

    @Override
    public void onSampleLost(DataReader<?> reader, SampleLostStatus status) {
        // no-op
    }
}
