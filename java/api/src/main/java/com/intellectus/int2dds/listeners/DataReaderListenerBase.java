package com.intellectus.int2dds.listeners;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.status.SubscriptionMatchedStatus;

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
}
