package kr.co.intellectus.int2dds.core;

import java.nio.ByteBuffer;

/** One element of a {@link LoanedSamples}: its SampleInfo and the lent serialized bytes. */
public final class LoanedSample {
    private final LoanedSamples<?> owner;
    private final SampleInfo info;
    private final ByteBuffer data;

    LoanedSample(LoanedSamples<?> owner, SampleInfo info, ByteBuffer data) {
        this.owner = owner;
        this.info = info;
        this.data = data;
    }

    public SampleInfo info() {
        owner.checkOpen();
        return info;
    }

    /**
     * Read-only view of the sample's CDR bytes in native memory, or {@code null} when {@code
     * info().validData()} is false. The view must not be used after the loan is returned.
     */
    public ByteBuffer serializedData() {
        owner.checkOpen();
        return data == null ? null : data.duplicate();
    }
}
