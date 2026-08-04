package com.intellectus.int2dds.bench;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import java.nio.ByteBuffer;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.*;
import org.openjdk.jmh.infra.Blackhole;

/**
 * Encodes a representative record and reports throughput.
 *
 * <p>The two candidates are compared end to end: the direct-buffer path stops
 * once the bytes are encoded, because they are already at a native address; the
 * byte[] path must additionally copy into a reusable direct staging buffer,
 * because a Java array has no stable native address for the FFI to read.
 * Leaving that copy out would compare the two unfairly.
 */
@BenchmarkMode(Mode.Throughput)
@OutputTimeUnit(TimeUnit.MICROSECONDS)
@State(Scope.Thread)
@Warmup(iterations = 5, time = 1)
@Measurement(iterations = 10, time = 1)
@Fork(2)
public class CdrEncodeBenchmark {

    /** Total payload size the record pads out to. */
    @Param({"64", "1024", "65536"})
    public int payloadBytes;

    private byte[] blob;
    private String name;

    // The byte[] candidate's staging buffer, reused across invocations exactly
    // as a real write path would.
    private byte[] heapBuffer;
    private ByteBuffer staging;

    @Setup(Level.Trial)
    public void setUp() {
        name = "sensor/temperature";
        int overhead = 64;
        blob = new byte[Math.max(0, payloadBytes - overhead)];
        for (int i = 0; i < blob.length; i++) {
            blob[i] = (byte) i;
        }
        heapBuffer = new byte[payloadBytes + 256];
        staging = ByteBuffer.allocateDirect(payloadBytes + 256);
    }

    /** Ten mixed fields plus the blob — the shape of a typical sample. */
    private static void encode(CdrWriter w, String name, byte[] blob) {
        w.writeI32(1);
        w.writeI64(2L);
        w.writeF64(3.5d);
        w.writeBool(true);
        w.writeI16((short) 4);
        w.writeU8(5);
        w.writeF32(6.5f);
        w.writeEnum(7);
        w.writeString(name);
        w.writeSeqHeader(blob.length);
        w.writeBytes(blob);
    }

    @Benchmark
    public long directBuffer() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            encode(w, name, blob);
            // Already at a native address: nothing further to do.
            return w.address() + w.length();
        }
    }

    @Benchmark
    public int heapThenStage(Blackhole bh) {
        // Models the byte[] candidate: encode into a heap array, then one bulk
        // copy into the reusable direct staging buffer the FFI would read.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            encode(w, name, blob);
            int len = w.length();
            ByteBuffer src = w.buffer();
            src.get(heapBuffer, 0, len);
            ((java.nio.Buffer) staging).clear();
            staging.put(heapBuffer, 0, len);
            bh.consume(staging);
            return len;
        }
    }
}
