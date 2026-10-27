package kr.co.intellectus.int2dds.core;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import kr.co.intellectus.int2dds.types.IDdsType;

/**
 * Samples loaned from a DataReader's cache without copying (DDS v1.4 2.2.2.5.3.20). {@link #close()}
 * or {@link DataReader#returnLoan} returns the loan; a reader with an outstanding loan cannot be
 * closed.
 */
public final class LoanedSamples<T extends IDdsType> implements AutoCloseable, Iterable<LoanedSample> {
    private final DataReader<T> reader;
    private final List<LoanedSample> samples = new ArrayList<LoanedSample>();
    private long handle;
    private boolean returned;

    LoanedSamples(DataReader<T> reader, long handle) {
        this.reader = reader;
        this.handle = handle;
    }

    public int size() {
        checkOpen();
        return samples.size();
    }

    public LoanedSample get(int index) {
        checkOpen();
        return samples.get(index);
    }

    @Override
    public Iterator<LoanedSample> iterator() {
        checkOpen();
        return Collections.unmodifiableList(samples).iterator();
    }

    /** Returns the loan to its reader. Idempotent. */
    @Override
    public void close() {
        reader.returnLoan(this);
    }

    void add(LoanedSample sample) {
        samples.add(sample);
    }

    long nativeHandle() {
        return handle;
    }

    boolean isReturned() {
        return returned;
    }

    void markReturned() {
        returned = true;
        handle = 0L;
    }

    void checkOpen() {
        if (returned) {
            throw new IllegalStateException("loan has been returned");
        }
    }
}
