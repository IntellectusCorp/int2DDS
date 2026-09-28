package kr.co.intellectus.int2dds.listeners;

import kr.co.intellectus.int2dds.core.DataWriter;
import kr.co.intellectus.int2dds.status.LivelinessLostStatus;
import kr.co.intellectus.int2dds.status.OfferedDeadlineMissedStatus;
import kr.co.intellectus.int2dds.status.OfferedIncompatibleQosStatus;
import kr.co.intellectus.int2dds.status.PublicationMatchedStatus;

/**
 * No-op {@link DataWriterListener}. Extend this and override only the callbacks
 * you need; later callbacks added to the interface land here as no-ops, so
 * subclasses stay source-compatible.
 */
public class DataWriterListenerBase implements DataWriterListener {

    @Override
    public void onPublicationMatched(DataWriter<?> writer, PublicationMatchedStatus status) {
        // no-op
    }

    @Override
    public void onOfferedDeadlineMissed(DataWriter<?> writer, OfferedDeadlineMissedStatus status) {
        // no-op
    }

    @Override
    public void onOfferedIncompatibleQos(DataWriter<?> writer, OfferedIncompatibleQosStatus status) {
        // no-op
    }

    @Override
    public void onLivelinessLost(DataWriter<?> writer, LivelinessLostStatus status) {
        // no-op
    }
}
