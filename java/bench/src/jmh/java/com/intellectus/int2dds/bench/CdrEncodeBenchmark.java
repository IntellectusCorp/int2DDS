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
 * <p><b>What this measures:</b> the cost of {@code encode(...)} against the
 * cost of {@code encode(...)} plus a copy-out into a heap array and a copy-in
 * to a reusable direct staging buffer — both arms encode into the same
 * direct-backed {@link CdrWriter}, because no {@code byte[]}-backed writer
 * exists in this codebase to measure directly.
 *
 * <p><b>What this does not measure:</b> a comparison of two backing stores.
 * {@code heapThenStage} is not a stand-in for a real {@code byte[]}-backed
 * writer — a real one would encode with plain array stores (cheaper than the
 * direct-buffer writes measured here) and pay a single copy into staging, not
 * an extra {@code duplicate()}/{@code asReadOnlyBuffer()}/{@code slice()} plus
 * two bulk copies. Because both arms share the identical direct-backed
 * encode and {@code heapThenStage} only ever adds work on top of it,
 * {@code heapThenStage} cannot beat {@code directBuffer} in this benchmark
 * regardless of how a genuine {@code byte[]} writer would perform. Do not
 * read a {@code directBuffer} win here as evidence that direct is faster than
 * a real {@code byte[]} design — that question is still open and needs an
 * actual {@code byte[]}-backed {@code CdrWriter} to answer.
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
    public int directBuffer(Blackhole bh) {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            encode(w, name, blob);
            // w.address() and w.length() do not depend on the bytes actually
            // stored: address is a field cached at allocation time and length
            // is a counter advanced by arithmetic independent of the store
            // instructions. Consuming a view object (an earlier version of
            // this benchmark did `bh.consume(w.buffer())`) does not help
            // either: Blackhole.consume(Object) never dereferences its
            // argument, so the view's bytes are never actually read and the
            // encode's stores remain eligible for elimination. The bulk
            // `get` below is what forces the read — it is the same copy
            // heapThenStage performs, so both arms pay for exactly one real
            // read of the encoded bytes. A per-byte checksum loop on top of
            // that would add cost the other arm does not pay, so consuming a
            // single element of the filled array is enough to keep the `get`
            // itself from being elided.
            int len = w.length();
            w.buffer().get(heapBuffer, 0, len);
            bh.consume(heapBuffer[0]);
            return len;
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
