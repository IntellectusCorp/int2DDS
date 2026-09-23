package com.intellectus.int2dds.core;

import java.nio.ByteBuffer;
import java.util.Arrays;

/**
 * Per-sample metadata, an immutable mirror of the core's {@code
 * Int2DdsSampleInfo} (ffi/src/types.rs). State fields carry the DDS bit masks
 * as raw ints; the 16-byte handles are copied on the way in and out.
 */
public final class SampleInfo {

    /** repr(C) size of Int2DdsSampleInfo; pinned by the java/native layout test. */
    static final int STRUCT_SIZE = 76;

    private final int sourceTimestampSec;
    private final int sourceTimestampNanosec;
    private final int sampleState;
    private final int viewState;
    private final int instanceState;
    private final byte[] instanceHandle;
    private final byte[] publicationHandle;
    private final int disposedGenerationCount;
    private final int noWritersGenerationCount;
    private final int sampleRank;
    private final int generationRank;
    private final int absoluteGenerationRank;
    private final boolean validData;

    SampleInfo(int sourceTimestampSec, int sourceTimestampNanosec, int sampleState,
            int viewState, int instanceState, byte[] instanceHandle, byte[] publicationHandle,
            int disposedGenerationCount, int noWritersGenerationCount, int sampleRank,
            int generationRank, int absoluteGenerationRank, boolean validData) {
        this.sourceTimestampSec = sourceTimestampSec;
        this.sourceTimestampNanosec = sourceTimestampNanosec;
        this.sampleState = sampleState;
        this.viewState = viewState;
        this.instanceState = instanceState;
        this.instanceHandle = Arrays.copyOf(instanceHandle, instanceHandle.length);
        this.publicationHandle = Arrays.copyOf(publicationHandle, publicationHandle.length);
        this.disposedGenerationCount = disposedGenerationCount;
        this.noWritersGenerationCount = noWritersGenerationCount;
        this.sampleRank = sampleRank;
        this.generationRank = generationRank;
        this.absoluteGenerationRank = absoluteGenerationRank;
        this.validData = validData;
    }

    /** Decodes the repr(C) struct written by the core into a value object. */
    static SampleInfo decode(ByteBuffer s) {
        byte[] inst = new byte[16];
        byte[] pub = new byte[16];
        for (int i = 0; i < 16; i++) {
            inst[i] = s.get(20 + i);
            pub[i] = s.get(36 + i);
        }
        return new SampleInfo(
                s.getInt(0), s.getInt(4), s.getInt(8), s.getInt(12), s.getInt(16),
                inst, pub, s.getInt(52), s.getInt(56), s.getInt(60), s.getInt(64),
                s.getInt(68), s.get(72) != 0);
    }

    public int sourceTimestampSec() {
        return sourceTimestampSec;
    }

    public int sourceTimestampNanosec() {
        return sourceTimestampNanosec;
    }

    public int sampleState() {
        return sampleState;
    }

    public int viewState() {
        return viewState;
    }

    public int instanceState() {
        return instanceState;
    }

    public byte[] instanceHandle() {
        return Arrays.copyOf(instanceHandle, instanceHandle.length);
    }

    public byte[] publicationHandle() {
        return Arrays.copyOf(publicationHandle, publicationHandle.length);
    }

    public int disposedGenerationCount() {
        return disposedGenerationCount;
    }

    public int noWritersGenerationCount() {
        return noWritersGenerationCount;
    }

    public int sampleRank() {
        return sampleRank;
    }

    public int generationRank() {
        return generationRank;
    }

    public int absoluteGenerationRank() {
        return absoluteGenerationRank;
    }

    public boolean validData() {
        return validData;
    }
}
