package com.intellectus.int2dds.bench;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.ReliabilityKind;
import java.nio.Buffer;
import java.nio.ByteBuffer;
import java.nio.charset.Charset;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.Fork;
import org.openjdk.jmh.annotations.Level;
import org.openjdk.jmh.annotations.Measurement;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Param;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.TearDown;
import org.openjdk.jmh.annotations.Warmup;
import org.openjdk.jmh.infra.Blackhole;

/**
 * Measures the two write-path questions Tasks 4-7 could not, because no write
 * path existed yet — S1 and V1, see
 * {@code docs/superpowers/specs/2026-08-05-java-core-write-design.md} §5.4.
 *
 * <p><b>S1 — what does pooling the encoder buffer buy?</b> {@link CdrWriter}
 * pools a direct {@link ByteBuffer} per thread ({@link
 * CdrWriter#acquire}/{@link CdrWriter#close}). {@link #directEncodeOnly}
 * measures that. {@link #directAllocEncodeOnly} measures the alternative — a
 * fresh {@code allocateDirect} per sample — by running the identical encode
 * but arranging for the pool to never actually supply a reused buffer; see
 * that method's own doc for exactly how and why that is a genuine, not
 * merely apparent, difference.
 *
 * <p><b>V1 — direct {@code ByteBuffer}, or {@code byte[]} plus a staging
 * copy?</b> The value-types branch left this open, and its own benchmark
 * ({@link CdrEncodeBenchmark}, see that class's Javadoc) could not settle
 * it: both its arms encoded through the same direct-backed {@code
 * CdrWriter}, so the "switch to {@code byte[]}" branch of the decision rule
 * was unreachable by construction — {@code heapThenStage} there was always
 * {@code directBuffer}'s own encode plus extra copying on top, which
 * guarantees {@code directBuffer} wins regardless of how a real {@code
 * byte[]} design would perform. This class builds the missing instrument:
 * {@link #heapEncodeOnly} encodes through {@link BenchOnlyHeapEncoder} —
 * plain array stores, no {@code ByteBuffer} — then copies once into a
 * reusable direct staging buffer, so the two V1 arms finally take genuinely
 * different routes to the same place.
 *
 * <p><b>Both arms end at the same place.</b> Per the brief: an address
 * handed to {@code int2dds_datawriter_write_serialized}. The encode-only
 * arms stop at that address (a pooled {@code CdrWriter}'s {@link
 * CdrWriter#address()}, or the reusable staging buffer's cached native
 * address); the encode-and-write arms go one step further and actually call
 * {@link FfiAccess#datawriterWriteSerialized} with it, through a real native
 * {@code DataWriter} — see {@link WriteTarget} — on a BEST_EFFORT topic with
 * no matched reader, so the call always returns immediately with nothing to
 * wait on.
 *
 * <p><b>Two measurement levels, because they can disagree.</b> Level 1 —
 * {@link #directEncodeOnly}/{@link #directAllocEncodeOnly}/{@link
 * #heapEncodeOnly} — isolates the encoder. Level 2 — {@link
 * #directEncodeAndWrite}/{@link #heapEncodeAndWrite} — adds the core's own
 * {@code write_serialized} work on top. If level 2 shows the level-1
 * difference shrinking or disappearing into the write's own cost, that is
 * the finding, and it is more actionable than the level-1 number alone: it
 * says the encoder choice does not matter once a real write is in the
 * picture, regardless of which encoder is faster in isolation.
 *
 * <h2>Defeating dead-code elimination</h2>
 *
 * <p>The previous branch's benchmark reported a 2.3x gap between two arms
 * that, per its own class Javadoc, could not actually differ — the gap was
 * never explained by anything the benchmark's names claimed to measure. Its
 * first attempt returned only {@code address()}/{@code length()}, neither of
 * which depends on whether the encoder's stores actually ran, so those
 * stores were eligible for elimination. Its fix, {@code
 * bh.consume(w.buffer())}, did not close that gap: {@code
 * Blackhole.consume(Object)} never dereferences its argument — it keeps the
 * reference reachable, nothing more — so consuming a {@code ByteBuffer}
 * <em>view</em> object still never reads the bytes inside it, and the
 * encode's stores remained just as eliminable as before. The gap that
 * measurement actually reported was indistinguishable from the allocation
 * cost of the three {@code ByteBuffer} objects {@link CdrWriter#buffer()}
 * builds on every call ({@code duplicate().asReadOnlyBuffer().slice()}) —
 * see {@code docs/superpowers/specs/2026-07-30-java-value-types-design.md}
 * §8.2's own account.
 *
 * <p>Both encode-only arms here instead do a real bulk {@code get} of the
 * encoded range into a scratch {@code byte[]}, then {@code
 * bh.consume(scratch[0])} — consuming a primitive {@code byte} value that
 * cannot exist unless the {@code get} actually ran, which in turn cannot
 * legally return the right bytes unless the encoder's stores actually ran.
 * That is what makes the stores non-eliminable, not the {@code consume}
 * call by itself.
 *
 * <p><b>That read-back must cost the same on both arms, or it becomes a new
 * confound.</b> {@link CdrWriter#buffer()} — the direct arm's only public
 * way to get a bounded, readable view of its pooled buffer — always builds
 * those three view objects; a naive {@code heapEncodeOnly} that instead read
 * straight off its own {@code staging} field would pay for none of them,
 * making any V1 result partly a comparison of read-back instrumentation
 * rather than encoders (exactly the confound that sank the previous
 * benchmark, see above). {@link #readOnlyRange} reproduces {@code
 * CdrWriter#buffer()}'s exact recipe against {@code staging} instead, so
 * {@link #heapEncodeOnly} pays the identical tax. That cost is then equal
 * across both arms at every payload size and cancels out of the difference
 * between them — it does not cancel out of either arm's own absolute
 * number, which is why level 1's absolute throughput is not directly
 * comparable to level 2's (a further reason the two levels are reported
 * separately rather than combined). <b>Concretely: level 1's score is
 * encode-plus-read-back, and level 2 never performs a read-back at all
 * (the native write call is what consumes the encoded bytes instead — see
 * below) — so {@code (1/L2score − 1/L1score)} is not the write's own added
 * cost, it is the write's added cost <em>minus</em> the read-back tax L1
 * paid and L2 didn't.</b> That subtraction is small enough to ignore at
 * small payloads, where the read-back tax is a few hundred nanoseconds at
 * most against a multi-microsecond write; it is not small at large
 * payloads, where the read-back tax is itself a bulk copy of the whole
 * payload and can be a substantial fraction of level 1's own score. Treat
 * any decomposition built this way as a bound, not an exact split, at
 * large payload sizes specifically — see the task report for the measured
 * size of this specific error at 64&nbsp;KB.
 *
 * <p>The encode-and-write arms need no such device: {@code
 * datawriterWriteSerialized} is a native (JNI) call, and unlike {@code
 * Blackhole.consume}, it genuinely dereferences the address it is given —
 * that is how it builds the outgoing RTPS message. A JIT cannot inline a
 * native method or see into its implementation, so it must conservatively
 * treat the call as capable of reading any memory reachable from its
 * arguments; the encoder's stores into that memory therefore cannot be
 * proven dead and cannot be hoisted past the call. This is the same
 * "opaque native call as an optimization barrier" reasoning this codebase's
 * own {@code NativeKeepAlive} relies on (see that class's Javadoc).
 *
 * <h2>Why {@link #directAllocEncodeOnly} and {@link #heapEncodeOnly} do not
 * need a {@code NativeKeepAlive} fence</h2>
 *
 * <p>{@code NativeKeepAlive} exists for a specific shape of hazard: a raw
 * {@code long} handle or address is read out of an object, and nothing else
 * in the method touches that object again before a native call that is
 * still using the value — so, once the object's last use has passed, the
 * JIT is free to treat it as unreachable and a concurrent GC could free its
 * backing memory while the native call is in flight. Nothing here has that
 * shape. The {@code CdrWriter} local {@code w} is used again, textually
 * after the address is read, by {@code w.buffer()} (or the implicit {@code
 * close()} the try-with-resources block reduces to) — it never becomes the
 * sole owner of an otherwise-dead reference before the native call
 * completes. {@code staging} and {@code heapEncoder} are fields on this
 * {@code @State} object, which JMH itself holds a strong reference to for
 * the whole trial; they cannot become unreachable — and so cannot have
 * their {@code Cleaner}-backed native memory freed — while any benchmark
 * method on this instance is still running. {@link WriteTarget}'s
 * participant/topic/publisher/writer are plain {@code long} handles created
 * and destroyed directly through {@link FfiAccess}, never wrapped in a
 * {@code NativeEntity}; with no {@code NativeEntity} there is no {@code
 * NativeCleaner} registration and so no reaper watching them at all — they
 * live and die exactly when {@link WriteTarget#setUp} and {@link
 * WriteTarget#tearDown} say so.
 */
@BenchmarkMode(Mode.Throughput)
@OutputTimeUnit(TimeUnit.MICROSECONDS)
@State(Scope.Thread)
@Warmup(iterations = 5, time = 1)
@Measurement(iterations = 10, time = 1)
@Fork(2)
public class WritePathBenchmark {

    /** Bytes around the padded string field: 4-byte encapsulation header +
     *  4-byte DHEADER + 4-byte i32 + 8-byte f64 + 4-byte string length +
     *  1-byte NUL terminator. The {@code @Param} value is the *total*
     *  encoded length, so the string is padded to {@code payloadBytes -
     *  FIXED_OVERHEAD} ASCII characters. */
    private static final int FIXED_OVERHEAD = 25;

    /** Slack past {@code payloadBytes} for every buffer this class owns, so
     *  no buffer ever needs to grow mid-trial (a growth event would mix
     *  reallocation cost into whichever invocation triggered it). */
    private static final int SLACK = 256;

    @Param({"64", "1024", "65536"})
    public int payloadBytes;

    private int id;
    private double value;
    private String label;

    // V1's heap arm and its reusable staging buffer -- see this class's own
    // doc for why staging's read-back must cost the same as the direct arm's.
    private BenchOnlyHeapEncoder heapEncoder;
    private ByteBuffer staging;
    // Cached once, like CdrWriter's own Pooled.address: directBufferAddress
    // is a JNI call, and recomputing it every invocation would give back
    // exactly what avoiding a per-sample copy is supposed to save.
    private long stagingAddr;

    // Scratch target for the DCE-defeating bulk read every encode-only arm
    // performs -- see this class's own "Defeating dead-code elimination" doc.
    private byte[] readBack;

    @Setup(Level.Trial)
    public void setUp() {
        id = 42;
        value = -12.5d;

        int labelLen = Math.max(0, payloadBytes - FIXED_OVERHEAD);
        StringBuilder sb = new StringBuilder(labelLen);
        for (int i = 0; i < labelLen; i++) {
            sb.append('x'); // ASCII padding -- BenchOnlyHeapEncoder has no other path.
        }
        label = sb.toString();

        int capacity = payloadBytes + SLACK;
        heapEncoder = new BenchOnlyHeapEncoder(capacity);
        staging = ByteBuffer.allocateDirect(capacity);
        stagingAddr = FfiAccess.directBufferAddress(staging);
        if (stagingAddr == 0L) {
            throw new IllegalStateException("allocateDirect did not produce a direct buffer");
        }
        readBack = new byte[capacity];
    }

    /** {@code ConformanceRecord.serializeCdr}'s own field sequence (see that
     *  class in {@code api}'s test sources), reproduced here directly rather
     *  than depending on a test-sourceset class the bench module cannot see:
     *  DHEADER, i32, f64, string. */
    private static void encodeDirect(CdrWriter w, int id, double value, String label) {
        int token = w.dheaderBegin();
        w.writeI32(id);
        w.writeF64(value);
        w.writeString(label);
        w.dheaderFinalize(token);
    }

    /**
     * Reproduces {@link CdrWriter#buffer()}'s exact view-construction recipe
     * against an arbitrary backing buffer, so a read-back off {@code
     * staging} costs what a read-back off a pooled {@code CdrWriter} costs.
     * See this class's own doc for why that symmetry matters.
     */
    private static ByteBuffer readOnlyRange(ByteBuffer backing, int len) {
        ByteBuffer view = backing.duplicate().asReadOnlyBuffer();
        ((Buffer) view).position(0);
        ((Buffer) view).limit(len);
        return view.slice();
    }

    /**
     * Reads {@code [0, len)} back from {@code source} — already positioned
     * at 0 with limit {@code len}, as {@link #readOnlyRange} and {@link
     * CdrWriter#buffer()} both return — into {@code scratch}, then consumes
     * one real byte of the result. See this class's own "Defeating
     * dead-code elimination" doc for why the bulk {@code get} is the part
     * that matters, not the {@code consume} by itself.
     */
    private static void consumeRange(ByteBuffer source, int len, byte[] scratch, Blackhole bh) {
        source.get(scratch, 0, len);
        bh.consume(scratch[0]);
    }

    /** S1 baseline / V1's direct arm: encode into the pooled direct {@code
     *  CdrWriter}, stop at its address. */
    @Benchmark
    public int directEncodeOnly(Blackhole bh) {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            encodeDirect(w, id, value, label);
            int len = w.length();
            consumeRange(w.buffer(), len, readBack, bh);
            return len;
        }
    }

    /**
     * S1's allocation arm: the same {@link #encodeDirect} call {@link
     * #directEncodeOnly} makes, but against a fresh {@code
     * ByteBuffer.allocateDirect} instead of the thread's pooled one.
     *
     * <p><b>How "fresh every time" is actually arranged, and why it is
     * genuine rather than assumed.</b> This method deliberately never calls
     * {@code close()} on {@code w} — no try-with-resources, no explicit
     * close. {@link CdrWriter}'s pool ({@code CdrWriter.POOL}) is a
     * per-thread {@code ArrayDeque} that gains an entry only inside {@code
     * close()} and loses one only inside {@code take()}'s {@code
     * pool.poll()}; nothing else touches it. A method that never calls
     * {@code close()} therefore never pushes anything into that deque for
     * this thread, so every {@code poll()} inside every {@code acquire()}
     * below — warmup and measurement alike — sees an empty deque and falls
     * through to {@code allocate()}: a fresh {@code ByteBuffer.allocateDirect}
     * plus the {@code directBufferAddress} JNI call, exactly the cost {@code
     * CdrWriter}'s own class doc says pooling exists to avoid paying per
     * sample.
     *
     * <p><b>That is one allocation only at the smallest payload size.</b>
     * {@code CdrWriter}'s constructor always calls {@code take(
     * DEFAULT_CAPACITY)} — 256 bytes — regardless of what the caller will
     * eventually write; {@code ensure()} then grows (a second {@code
     * allocate()}, sized to the actual requirement, plus a copy of the bytes
     * already written) the moment the running length would exceed whatever
     * is currently held. At 64&nbsp;B the initial 256-byte buffer is never
     * exceeded, so this method allocates once. At 1024&nbsp;B and 65536&nbsp;B
     * it allocates twice per invocation, not once — the mechanism above
     * ("every acquire() falls through to allocate()") is accurate but does
     * not by itself say how many times. See the task report for measured
     * per-call allocation costs at each size and what this means for the
     * S1 multipliers at those two sizes specifically.
     *
     * <p>This is deterministic, not probabilistic: {@code ArrayDeque} and
     * {@code ThreadLocal} have no hidden cross-talk, and JMH forks a fresh
     * JVM process per (benchmark method, {@code @Param} value) trial by
     * default, so this method's dedicated fork never ran any other
     * benchmark first that could have left something in this thread's pool
     * for it to find. (Even if it somehow had — at most {@code POOL_LIMIT =
     * 4} entries could ever be sitting there, a rounding error against the
     * tens of thousands of invocations one second of throughput-mode
     * measurement performs.) Reusing {@link #encodeDirect} rather than
     * hand-rolling a second copy of the encode also means the only variable
     * between this method and {@link #directEncodeOnly} is genuinely pooled
     * vs. fresh — not two independently-written encoders that could differ
     * for reasons unrelated to pooling.
     *
     * <p>Missing a {@code close()} here is deliberate, not a leak: {@code
     * CdrWriter}'s own class doc says exactly this — the buffer never
     * returns to the pool and the JVM reclaims it once unreachable, which
     * for this method is true of every single invocation's buffer.
     */
    @Benchmark
    public int directAllocEncodeOnly(Blackhole bh) {
        CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true);
        encodeDirect(w, id, value, label);
        int len = w.length();
        consumeRange(w.buffer(), len, readBack, bh);
        return len;
    }

    /** V1's heap arm: encode into {@link BenchOnlyHeapEncoder}'s owned
     *  {@code byte[]}, copy once into the reusable direct staging buffer,
     *  stop at its address. */
    @Benchmark
    public int heapEncodeOnly(Blackhole bh) {
        heapEncoder.reset();
        heapEncoder.encode(id, value, label);
        int len = heapEncoder.length();
        ((Buffer) staging).clear();
        heapEncoder.copyInto(staging);
        consumeRange(readOnlyRange(staging, len), len, readBack, bh);
        return len;
    }

    /** Level 2 direct arm: {@link #directEncodeOnly}'s encode, then a real
     *  {@code write_serialized} through {@link WriteTarget}'s writer. */
    @Benchmark
    public int directEncodeAndWrite(WriteTarget target, Blackhole bh) {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            encodeDirect(w, id, value, label);
            int rc = FfiAccess.datawriterWriteSerialized(
                    target.writer, w.address(), w.length());
            if (rc != 0) {
                throw new IllegalStateException("write_serialized failed: rc=" + rc);
            }
            bh.consume(rc);
            return rc;
        }
    }

    /** Level 2 heap arm: {@link #heapEncodeOnly}'s encode-then-stage, then a
     *  real {@code write_serialized} through {@link WriteTarget}'s writer,
     *  off the staging buffer's cached address rather than the pooled
     *  {@code CdrWriter}'s. */
    @Benchmark
    public int heapEncodeAndWrite(WriteTarget target, Blackhole bh) {
        heapEncoder.reset();
        heapEncoder.encode(id, value, label);
        int len = heapEncoder.length();
        ((Buffer) staging).clear();
        heapEncoder.copyInto(staging);
        int rc = FfiAccess.datawriterWriteSerialized(target.writer, stagingAddr, len);
        if (rc != 0) {
            throw new IllegalStateException("write_serialized failed: rc=" + rc);
        }
        bh.consume(rc);
        return rc;
    }

    /**
     * A real native participant/topic/publisher/DataWriter, built directly
     * through {@link FfiAccess} rather than the public {@code
     * DomainParticipant}/{@code Topic}/{@code Publisher}/{@code DataWriter}
     * wrapper classes: those classes' {@code handle()} is package-private to
     * {@code com.intellectus.int2dds.core} (see {@code NativeEntity}), so
     * this package cannot reach the raw handle {@code
     * write_serialized} needs regardless — and the encode-and-write
     * benchmarks specifically need to choose which address reaches that
     * call (the pooled {@code CdrWriter}'s or the staging buffer's), which
     * the typed {@code DataWriter<T>.write(T)} API does not expose: it
     * always serializes into, and publishes the address of, its own
     * internal pooled writer. Built the same raw-{@code FfiAccess} way
     * {@code WritePathEndToEndTest} already builds its receive-side
     * subscriber/reader.
     *
     * <p>A separate {@code @State} class, not fields on {@link
     * WritePathBenchmark} itself, so that only the two benchmark methods
     * that declare it as a parameter — {@link #directEncodeAndWrite} and
     * {@link #heapEncodeAndWrite} — ever pay participant/writer setup cost
     * or run alongside their background threads (SPDP, lease renewal). JMH
     * only instantiates and sets up a {@code @State} parameter for the
     * benchmark methods that actually declare it; the three encode-only
     * methods above do not, so they never build this tree at all.
     *
     * <p>BEST_EFFORT, explicitly — not left to whatever the core's own
     * default happens to be — and never matched to a reader (no subscriber
     * or reader is created anywhere in this class), so every {@code
     * write_serialized} call returns immediately with nothing to wait on or
     * retransmit for. The topic is effectively unkeyed (no key fields are
     * registered), so every sample lands in the same single instance and
     * the writer's own history queue cannot grow across the trial's
     * millions of invocations regardless of reliability kind.
     */
    @State(Scope.Thread)
    public static class WriteTarget {

        private static final Charset UTF8 = Charset.forName("UTF-8");

        private long participant;
        private long topic;
        private long publisher;
        private long writer;

        @Setup(Level.Trial)
        public void setUp() {
            // Same [100, 200) loopback-forced domain convention
            // java/api/build.gradle.kts's Test tasks use, resolved through a
            // system property rather than the DEFAULT_DOMAIN_ID (-1) sentinel
            // -- the same choice every other test in this codebase that needs
            // an isolated domain makes (see DomainParticipantTest.testDomain).
            int domainId = Integer.parseInt(System.getProperty("int2dds.bench.domain", "150"));

            long factory = FfiAccess.participantFactoryGetInstance();
            if (factory == 0L) {
                throw new IllegalStateException("participantFactoryGetInstance() returned 0");
            }
            long[] out = new long[1];
            check(FfiAccess.createParticipant(factory, domainId, 0L, out), "createParticipant");
            participant = out[0];

            check(FfiAccess.createTopic(participant, utf8("bench_write_path"),
                    utf8("BenchWritePathRecord"), Extensibility.APPENDABLE.value(), 0L, out),
                    "createTopic");
            topic = out[0];

            check(FfiAccess.createPublisher(participant, 0L, out), "createPublisher");
            publisher = out[0];

            long qos = FfiAccess.createDataWriterQos();
            if (qos == 0L) {
                throw new IllegalStateException("createDataWriterQos() returned 0");
            }
            try {
                check(FfiAccess.writerQosSetReliability(
                        qos, ReliabilityKind.BEST_EFFORT.value(), 100_000_000L),
                        "writerQosSetReliability");
                check(FfiAccess.createDataWriter(publisher, topic, qos, 0L, 0, out),
                        "createDataWriter");
                writer = out[0];
            } finally {
                FfiAccess.destroyDataWriterQos(qos);
            }
        }

        /** Reverse of creation order: a child must be gone before the native
         *  layer allows its parent to delete, exactly the constraint {@code
         *  NativeEntity.close()}'s own doc describes for the typed wrapper
         *  classes, enforced by hand here since none of those are in use. */
        @TearDown(Level.Trial)
        public void tearDown() {
            if (writer != 0L) {
                check(FfiAccess.deleteDataWriter(writer), "deleteDataWriter");
            }
            if (publisher != 0L) {
                check(FfiAccess.deletePublisher(publisher), "deletePublisher");
            }
            if (topic != 0L) {
                check(FfiAccess.deleteTopic(topic), "deleteTopic");
            }
            if (participant != 0L) {
                check(FfiAccess.deleteParticipant(participant), "deleteParticipant");
            }
        }

        private static void check(int rc, String what) {
            if (rc != 0) {
                throw new IllegalStateException(what + " failed: rc=" + rc);
            }
        }

        private static byte[] utf8(String s) {
            return s.getBytes(UTF8);
        }
    }
}
