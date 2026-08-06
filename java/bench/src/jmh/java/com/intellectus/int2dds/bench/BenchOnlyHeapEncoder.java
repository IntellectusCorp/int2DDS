package com.intellectus.int2dds.bench;

import java.nio.ByteBuffer;

/**
 * A minimal, bench-only CDR encoder over a heap {@code byte[]}.
 *
 * <p><b>This class exists only to answer benchmark question V1</b> for {@link
 * WritePathBenchmark} — "direct {@code ByteBuffer} or {@code byte[]} plus a
 * staging copy?" (see that class's own Javadoc, and §5.4 of
 * {@code docs/superpowers/specs/2026-08-05-java-core-write-design.md}). It
 * encodes only the three fields the benchmark's record needs — an {@code
 * int}, a {@code double}, an ASCII {@code String} — wrapped in the same
 * DHEADER and 4-byte encapsulation header {@link
 * com.intellectus.int2dds.cdr.CdrWriter} produces for an APPENDABLE, XCDR2,
 * little-endian sample (see {@code encode}'s own doc for the byte-for-byte
 * mapping). <b>It is not a second production CDR encoder</b>: no other
 * primitive, no big-endian path, no non-ASCII string path, no XCDR1 path —
 * because the one benchmark that owns it needs none of those. If the
 * byte[]-plus-copy design this class stands in for turns out to win, porting
 * an actual {@code byte[]}-backed {@code CdrWriter} is a separate decision
 * this class does not make and is not evidence for.
 *
 * <p>Deliberately not {@code CdrWriter}-shaped: no {@code acquire()}/{@code
 * close()}, no thread-local pool. A single instance is created once per
 * benchmark trial and reused across invocations via {@link #reset()},
 * mirroring how {@code CdrWriter}'s pooled buffer is reused across {@code
 * acquire()} calls — so neither V1 arm's number is inflated by allocation
 * neither design actually requires.
 *
 * <p>Package-private: nothing outside {@code WritePathBenchmark} — the only
 * thing V1 is about — has any business constructing one of these.
 */
final class BenchOnlyHeapEncoder {

    // ENCAP_D_CDR2_LE: APPENDABLE + XCDR2 + little-endian -- the one
    // configuration WritePathBenchmark's direct arm exercises (see
    // CdrWriter.writeEncapsulationHeader). Written big-endian on the wire
    // regardless of payload endianness, per that method's own comment.
    private static final int ENCAP_D_CDR2_LE = 0x0009;

    // encap(4) + DHEADER(4) + i32(4) + f64(8) + string-length(4): every field
    // up to the string is already a multiple of 4 bytes, so align4() below is
    // structurally a no-op for this exact field sequence -- included anyway
    // (rather than assumed away) because "same alignment rules" is the point.
    private static final int HEADER_LEN = 4;

    private byte[] buf;
    private int pos;

    BenchOnlyHeapEncoder(int initialCapacity) {
        buf = new byte[initialCapacity];
    }

    /** Rewinds to empty. Does not shrink or clear the backing array — the
     *  next {@link #encode} overwrites every byte it needs and nothing reads
     *  ahead of {@link #length()}, so stale bytes past the old length are
     *  never observed. */
    void reset() {
        pos = 0;
    }

    /** Bytes written since the last {@link #reset()}. */
    int length() {
        return pos;
    }

    /**
     * Copies {@code [0, length())} into {@code dest} at its current
     * position, advancing it — exactly the single bulk copy a real
     * {@code byte[]}-backed writer would pay to reach a native-addressable
     * staging buffer. The caller owns {@code dest}'s position/limit
     * management (see {@code WritePathBenchmark}'s use of {@code clear()}
     * beforehand).
     */
    void copyInto(ByteBuffer dest) {
        dest.put(buf, 0, pos);
    }

    /**
     * Encodes {@code (id, value, label)} as DHEADER, i32, f64, string —
     * {@code ConformanceRecord}'s own field sequence (see that class in
     * {@code api}'s test sources, and {@code CdrConformanceTest}) — with
     * plain array stores, no {@code ByteBuffer} anywhere on the write side.
     *
     * <p>Byte-for-byte, this reproduces what {@code CdrWriter.acquire(
     * Extensibility.APPENDABLE, true, true)} followed by {@code
     * dheaderBegin()}/{@code writeI32}/{@code writeF64}/{@code
     * writeString}/{@code dheaderFinalize} produces: a 4-byte encapsulation
     * header ({@code 0x00 0x09 0x00 0x00}), a 4-byte DHEADER back-patched
     * with the length of everything after it, the {@code int} and {@code
     * double} each aligned to 4 bytes (XCDR2 caps the normal 8-byte {@code
     * double} alignment at 4 — see {@code CdrWriter.align}'s own doc), and
     * the string as a 4-byte length (including the NUL terminator) followed
     * by the ASCII bytes and the NUL.
     *
     * @param label must be ASCII; the benchmark only ever pads with ASCII,
     *     so that is the only string path this class implements
     * @throws IllegalArgumentException if {@code label} contains a non-ASCII
     *     character
     */
    void encode(int id, double value, String label) {
        writeEncapsulationHeader();

        align4(); // no-op here (pos is already 4-aligned right after the
                   // header) -- see dheaderBegin's own align(4), reproduced
                   // for fidelity rather than assumed away.
        int dheaderPos = pos;
        ensureCapacity(4);
        pos += 4; // DHEADER placeholder. Nothing reads these 4 bytes before
                  // the unconditional back-patch below overwrites all of
                  // them, so -- unlike CdrWriter.dheaderBegin, which writes
                  // an explicit zero for a caller that might abandon the
                  // member without finalizing -- this private, always-paired
                  // method has no such caller and does not need to.

        writeI32(id);
        writeF64(value);
        writeAsciiString(label);

        putIntLE(dheaderPos, pos - dheaderPos - 4);
    }

    private void writeEncapsulationHeader() {
        ensureCapacity(4);
        buf[pos] = (byte) (ENCAP_D_CDR2_LE >>> 8);
        buf[pos + 1] = (byte) ENCAP_D_CDR2_LE;
        buf[pos + 2] = 0;
        buf[pos + 3] = 0;
        pos += 4;
    }

    private void writeI32(int v) {
        align4();
        ensureCapacity(4);
        putIntLE(pos, v);
        pos += 4;
    }

    private void writeF64(double v) {
        // XCDR2 caps alignment at 4 even for an 8-byte value -- CdrWriter's
        // own rule (Math.min(alignment, 4) in CdrWriter.align), reproduced
        // here since this class does not call CdrWriter.
        align4();
        ensureCapacity(8);
        putLongLE(pos, Double.doubleToRawLongBits(v));
        pos += 8;
    }

    private void writeAsciiString(String s) {
        int n = s.length();
        align4();
        ensureCapacity(4);
        putIntLE(pos, n + 1); // length includes the NUL terminator
        pos += 4;
        ensureCapacity(n + 1);
        for (int i = 0; i < n; i++) {
            char c = s.charAt(i);
            if (c >= 0x80) {
                throw new IllegalArgumentException(
                        "BenchOnlyHeapEncoder only encodes ASCII strings; found U+"
                                + Integer.toHexString(c) + " at index " + i);
            }
            buf[pos + i] = (byte) c;
        }
        buf[pos + n] = 0;
        pos += n + 1;
    }

    /**
     * Pads to the next 4-byte boundary measured from the start of the
     * encapsulated stream (after the 4-byte encapsulation header) — the same
     * rule {@code CdrWriter.align} applies, specialized to the fixed 4-byte
     * cap XCDR2 always uses (this class has no XCDR1 path to need the
     * uncapped case).
     */
    private void align4() {
        int streamPos = pos - HEADER_LEN;
        int aligned = (streamPos + 3) & ~3;
        int padding = aligned - streamPos;
        if (padding > 0) {
            ensureCapacity(padding);
            for (int i = 0; i < padding; i++) {
                buf[pos + i] = 0;
            }
            pos += padding;
        }
    }

    private void putIntLE(int off, int v) {
        buf[off] = (byte) v;
        buf[off + 1] = (byte) (v >>> 8);
        buf[off + 2] = (byte) (v >>> 16);
        buf[off + 3] = (byte) (v >>> 24);
    }

    private void putLongLE(int off, long v) {
        for (int i = 0; i < 8; i++) {
            buf[off + i] = (byte) (v >>> (8 * i));
        }
    }

    /** Doubles capacity until {@code additional} more bytes fit past {@code
     *  pos}, copying the live prefix forward. No upper cap: unlike {@code
     *  CdrWriter} this is never handed attacker-controlled sizes, only the
     *  benchmark's own {@code @Param} values, so the growth ceiling {@code
     *  CdrWriter.MAX_CAPACITY} exists for is not this class's problem. In
     *  practice {@code WritePathBenchmark} presizes the initial capacity to
     *  the trial's {@code payloadBytes} plus slack, so this never actually
     *  runs during a measured invocation. */
    private void ensureCapacity(int additional) {
        int required = pos + additional;
        if (required <= buf.length) {
            return;
        }
        int capacity = buf.length;
        while (capacity < required) {
            capacity *= 2;
        }
        byte[] bigger = new byte[capacity];
        System.arraycopy(buf, 0, bigger, 0, pos);
        buf = bigger;
    }
}
