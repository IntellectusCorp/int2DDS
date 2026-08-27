package com.intellectus.int2dds.cdr;

import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.Buffer;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import java.util.ArrayDeque;

/**
 * CDR encoder over a pooled direct {@link ByteBuffer}.
 *
 * <p>The buffer is direct so the serialized bytes already sit at a native
 * address and the FFI write needs no copy. Direct allocation is expensive
 * (malloc plus cleaner registration), so buffers are pooled per thread and
 * handed back by {@link #close()}.
 *
 * <p>Missing a {@code close()} costs performance, not correctness: the buffer
 * simply does not return to the pool and the JVM reclaims it when the reference
 * drops. Use try-with-resources.
 */
public final class CdrWriter implements AutoCloseable {

    /** Growth ceiling. A malformed or hostile sample must not be able to
     *  consume unbounded native memory. */
    public static final int MAX_CAPACITY = 64 * 1024 * 1024;

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private static final int DEFAULT_CAPACITY = 256;
    private static final int POOL_LIMIT = 4;

    // Encapsulation ids, big-endian on the wire.
    private static final int ENCAP_CDR_BE = 0x0000;
    private static final int ENCAP_CDR_LE = 0x0001;
    private static final int ENCAP_PL_CDR_BE = 0x0002;
    private static final int ENCAP_PL_CDR_LE = 0x0003;
    private static final int ENCAP_CDR2_BE = 0x0006;
    private static final int ENCAP_CDR2_LE = 0x0007;
    private static final int ENCAP_D_CDR2_BE = 0x0008;
    private static final int ENCAP_D_CDR2_LE = 0x0009;
    private static final int ENCAP_PL_CDR2_BE = 0x000A;
    private static final int ENCAP_PL_CDR2_LE = 0x000B;

    private static final ThreadLocal<ArrayDeque<Pooled>> POOL =
            new ThreadLocal<ArrayDeque<Pooled>>() {
                @Override
                protected ArrayDeque<Pooled> initialValue() {
                    return new ArrayDeque<Pooled>();
                }
            };

    /** A direct buffer paired with its native address. The address is cached
     *  because {@code directBufferAddress} is a JNI call — recomputing it per
     *  sample would give back what the zero-copy buffer saves. */
    private static final class Pooled {
        ByteBuffer buffer;
        long address;
    }

    private Pooled pooled;
    private int pos;
    private int headerSize;
    private final boolean littleEndian;
    private final boolean xcdr2;

    private CdrWriter(boolean littleEndian, boolean xcdr2) {
        this.littleEndian = littleEndian;
        this.xcdr2 = xcdr2;
        this.pooled = take(DEFAULT_CAPACITY);
        this.pooled.buffer.order(littleEndian ? ByteOrder.LITTLE_ENDIAN : ByteOrder.BIG_ENDIAN);
        this.pos = 0;
        this.headerSize = 0;
    }

    /** A writer that starts with the 4-byte encapsulation header. */
    public static CdrWriter acquire(Extensibility extensibility, boolean littleEndian,
            boolean xcdr2) {
        CdrWriter w = new CdrWriter(littleEndian, xcdr2);
        w.writeEncapsulationHeader(extensibility);
        return w;
    }

    /** A writer with no encapsulation header, for key serialization. */
    public static CdrWriter acquireRaw(boolean littleEndian, boolean xcdr2) {
        return new CdrWriter(littleEndian, xcdr2);
    }

    public int length() {
        return pos;
    }

    public boolean isXcdr2() {
        return xcdr2;
    }

    /** Native address of the encoded bytes. Stable until the next write that grows. */
    public long address() {
        return pooled.address;
    }

    /** A read-only view over {@code [0, length())}. Does not copy. */
    public ByteBuffer buffer() {
        ByteBuffer view = pooled.buffer.duplicate().asReadOnlyBuffer();
        ((Buffer) view).position(0);
        ((Buffer) view).limit(pos);
        return view.slice();
    }

    /** A copy of the encoded bytes. Allocates — never call this on a hot path. */
    public byte[] toBytes() {
        byte[] out = new byte[pos];
        ByteBuffer dup = pooled.buffer.duplicate();
        ((Buffer) dup).position(0);
        ((Buffer) dup).limit(pos);
        dup.get(out);
        return out;
    }

    @Override
    public void close() {
        if (pooled == null) {
            return;
        }
        ArrayDeque<Pooled> pool = POOL.get();
        if (pool.size() < POOL_LIMIT) {
            pool.push(pooled);
        }
        pooled = null;
    }

    // ---- buffer management ---------------------------------------------

    private static Pooled take(int minCapacity) {
        ArrayDeque<Pooled> pool = POOL.get();
        Pooled p = pool.poll();
        if (p != null && p.buffer.capacity() >= minCapacity) {
            return p;
        }
        if (p != null) {
            // Too small: drop it and allocate. Its memory is reclaimed normally.
            p = null;
        }
        return allocate(Math.max(minCapacity, DEFAULT_CAPACITY));
    }

    private static Pooled allocate(int capacity) {
        Pooled p = new Pooled();
        p.buffer = ByteBuffer.allocateDirect(capacity);
        p.address = FfiAccess.directBufferAddress(p.buffer);
        if (p.address == 0L) {
            throw new CdrException("allocateDirect did not produce a direct buffer");
        }
        return p;
    }

    /** Guarantees {@code additional} more bytes past {@code pos}. */
    private void ensure(int additional) {
        long required = (long) pos + additional;
        if (required > MAX_CAPACITY) {
            throw new CdrOverflowException(
                    "CDR writer would exceed the " + MAX_CAPACITY + " byte cap: " + required);
        }
        if (required <= pooled.buffer.capacity()) {
            return;
        }
        int capacity = pooled.buffer.capacity();
        while (capacity < required) {
            capacity = (int) Math.min((long) capacity * 2, MAX_CAPACITY);
        }
        Pooled bigger = allocate(capacity);
        bigger.buffer.order(pooled.buffer.order());
        ByteBuffer src = pooled.buffer.duplicate();
        ((Buffer) src).position(0);
        ((Buffer) src).limit(pos);
        ((Buffer) bigger.buffer).position(0);
        bigger.buffer.put(src);
        ((Buffer) bigger.buffer).position(0);
        pooled = bigger;
    }

    /**
     * Writes {@code count} zero bytes at {@code pos} and advances.
     *
     * <p>Required, not cosmetic. A pooled buffer arrives holding the previous
     * sample's bytes; any span the encoder reserves but does not fill —
     * alignment padding, a back-patched header — would otherwise carry that
     * content onto the wire.
     */
    private void zeroFill(int count) {
        ensure(count);
        ByteBuffer b = pooled.buffer;
        for (int i = 0; i < count; i++) {
            b.put(pos + i, (byte) 0);
        }
        pos += count;
    }

    private void putBulk(byte[] src, int off, int len) {
        ensure(len);
        ByteBuffer b = pooled.buffer;
        ((Buffer) b).position(pos);
        b.put(src, off, len);
        ((Buffer) b).position(0);
        pos += len;
    }

    // ---- alignment ------------------------------------------------------

    /**
     * Pads to the next {@code alignment} boundary measured from the start of
     * the encapsulated stream, not from the start of the buffer — the 4-byte
     * encapsulation header does not count toward alignment.
     *
     * <p>XCDR2 caps the maximum alignment at 4, so an 8-byte value aligns to 4.
     */
    private void align(int alignment) {
        if (alignment <= 1) {
            return;
        }
        int actual = xcdr2 ? Math.min(alignment, 4) : alignment;
        int streamPos = pos - headerSize;
        int aligned = (streamPos + actual - 1) & ~(actual - 1);
        int padding = aligned - streamPos;
        if (padding > 0) {
            zeroFill(padding);
        }
    }

    // ---- primitives -----------------------------------------------------

    public void writeBool(boolean value) {
        ensure(1);
        pooled.buffer.put(pos, value ? (byte) 1 : (byte) 0);
        pos += 1;
    }

    public void writeI8(byte value) {
        ensure(1);
        pooled.buffer.put(pos, value);
        pos += 1;
    }

    /** Writes the low 8 bits of {@code value}. */
    public void writeU8(int value) {
        writeI8((byte) value);
    }

    public void writeI16(short value) {
        align(2);
        ensure(2);
        pooled.buffer.putShort(pos, value);
        pos += 2;
    }

    /** Writes the low 16 bits of {@code value}. */
    public void writeU16(int value) {
        writeI16((short) value);
    }

    public void writeI32(int value) {
        align(4);
        ensure(4);
        pooled.buffer.putInt(pos, value);
        pos += 4;
    }

    /** Writes all 32 bits of {@code value}; the caller supplies the bit pattern. */
    public void writeU32(int value) {
        writeI32(value);
    }

    public void writeI64(long value) {
        align(8);
        ensure(8);
        pooled.buffer.putLong(pos, value);
        pos += 8;
    }

    /** Writes all 64 bits of {@code value}; the caller supplies the bit pattern. */
    public void writeU64(long value) {
        writeI64(value);
    }

    public void writeF32(float value) {
        writeI32(Float.floatToRawIntBits(value));
    }

    public void writeF64(double value) {
        writeI64(Double.doubleToRawLongBits(value));
    }

    // ---- strings, sequences, raw bytes -----------------------------------

    /**
     * Writes a CDR string: a uint32 length that includes the terminator, the
     * UTF-8 bytes, then a NUL.
     *
     * <p>Length counts bytes, not characters. An interior NUL is preserved —
     * CDR strings are length-prefixed, so it is legal payload.
     */
    public void writeString(String s) {
        if (s == null) {
            s = "";
        }
        int n = s.length();
        int ascii = asciiPrefixLength(s, n);
        if (ascii == n) {
            // Fast path: one byte per char, no intermediate array.
            writeU32(n + 1);
            ensure(n + 1);
            ByteBuffer b = pooled.buffer;
            for (int i = 0; i < n; i++) {
                b.put(pos + i, (byte) s.charAt(i));
            }
            b.put(pos + n, (byte) 0);
            pos += n + 1;
            return;
        }
        byte[] utf8 = s.getBytes(UTF8);
        writeU32(utf8.length + 1);
        putBulk(utf8, 0, utf8.length);
        ensure(1);
        pooled.buffer.put(pos, (byte) 0);
        pos += 1;
    }

    /** Index of the first char at or above U+0080, or {@code n} if all ASCII. */
    private static int asciiPrefixLength(String s, int n) {
        for (int i = 0; i < n; i++) {
            if (s.charAt(i) >= 0x80) {
                return i;
            }
        }
        return n;
    }

    /**
     * Writes a CDR wstring: a uint32 count of UTF-16 code units, then each unit
     * as a u16. Unlike {@link #writeString}, there is no terminator and the
     * count does not include one -- {@code String.length()} is already the
     * UTF-16 unit count, so a surrogate pair contributes two.
     */
    public void writeWString(String s) {
        if (s == null) {
            s = "";
        }
        int n = s.length();
        writeU32(n);
        for (int i = 0; i < n; i++) {
            writeU16(s.charAt(i));
        }
    }

    /** Writes a sequence header: the element count as a uint32. */
    public void writeSeqHeader(int count) {
        writeU32(count);
    }

    /** Writes raw bytes with no alignment and no length prefix. */
    public void writeBytes(byte[] data) {
        writeBytes(data, 0, data.length);
    }

    /** Writes raw bytes with no alignment and no length prefix. */
    public void writeBytes(byte[] data, int offset, int length) {
        putBulk(data, offset, length);
    }

    /** Writes an enum discriminant as a signed 32-bit integer. */
    public void writeEnum(int discriminant) {
        writeI32(discriminant);
    }

    // ---- encapsulation header -------------------------------------------

    private void writeEncapsulationHeader(Extensibility extensibility) {
        int encapId;
        if (xcdr2) {
            switch (extensibility) {
                case FINAL:
                    encapId = littleEndian ? ENCAP_CDR2_LE : ENCAP_CDR2_BE;
                    break;
                case MUTABLE:
                    encapId = littleEndian ? ENCAP_PL_CDR2_LE : ENCAP_PL_CDR2_BE;
                    break;
                case APPENDABLE:
                default:
                    encapId = littleEndian ? ENCAP_D_CDR2_LE : ENCAP_D_CDR2_BE;
                    break;
            }
        } else if (extensibility == Extensibility.MUTABLE) {
            // XCDR1 mutable is PL_CDR with PID member headers, not PLAIN_CDR.
            encapId = littleEndian ? ENCAP_PL_CDR_LE : ENCAP_PL_CDR_BE;
        } else {
            encapId = littleEndian ? ENCAP_CDR_LE : ENCAP_CDR_BE;
        }

        ensure(4);
        ByteBuffer b = pooled.buffer;
        // Always big-endian, independent of the payload's byte order.
        b.put(pos, (byte) (encapId >>> 8));
        b.put(pos + 1, (byte) encapId);
        b.put(pos + 2, (byte) 0);
        b.put(pos + 3, (byte) 0);
        pos += 4;
        headerSize = 4;
    }

    // ---- XCDR2 aggregates ------------------------------------------------

    private static final int MEMBER_ID_SENTINEL = 0x3F02;
    private static final int MAX_MEMBER_ID = 0x0FFFFFFF;

    private void requireXcdr2(String what) {
        if (!xcdr2) {
            throw new IllegalStateException(what + " is XCDR2-only; this writer is XCDR1. "
                    + "XCDR1 mutable types use PL_CDR PID member headers, not EMHEADER.");
        }
    }

    private static void requireMemberId(int memberId) {
        if (memberId < 0 || memberId > MAX_MEMBER_ID) {
            throw new IllegalArgumentException(
                    "member id exceeds 28 bits: 0x" + Integer.toHexString(memberId));
        }
    }

    /**
     * Reserves a DHEADER and returns a token for {@link #dheaderFinalize(int)}.
     * Returns {@code -1} under XCDR1, where there is no DHEADER; passing that
     * token back is a no-op, so callers need no version branch.
     */
    public int dheaderBegin() {
        if (!xcdr2) {
            return -1;
        }
        align(4);
        int token = pos;
        writeU32(0);
        return token;
    }

    /** Back-patches the DHEADER with the size of everything written since. */
    public void dheaderFinalize(int token) {
        if (!xcdr2) {
            return;
        }
        // Excludes the DHEADER word itself.
        pooled.buffer.putInt(token, pos - token - 4);
    }

    /** Writes an EMHEADER with a already-known data length, in NEXTINT form. */
    public void writeEmheader(int memberId, int dataLength, boolean mustUnderstand) {
        requireXcdr2("EMHEADER");
        requireMemberId(memberId);
        writeU32(emheaderWord(memberId, mustUnderstand));
        writeU32(dataLength);
    }

    /**
     * Writes an EMHEADER whose data length is not yet known, reserving the
     * NEXTINT word. Returns a token for {@link #emheaderFinalize(int)}.
     */
    public int emheaderBegin(int memberId, boolean mustUnderstand) {
        requireXcdr2("EMHEADER");
        requireMemberId(memberId);
        writeU32(emheaderWord(memberId, mustUnderstand));
        int token = pos;
        writeU32(0);
        return token;
    }

    /** Back-patches an EMHEADER with the length of the member that followed. */
    public void emheaderFinalize(int token) {
        pooled.buffer.putInt(token, pos - token - 4);
    }

    /** Writes the sentinel that terminates an XCDR2 mutable aggregate. */
    public void writeSentinel() {
        requireXcdr2("Sentinel");
        writeU32(MEMBER_ID_SENTINEL);
    }

    private static int emheaderWord(int memberId, boolean mustUnderstand) {
        int mu = mustUnderstand ? 0x80000000 : 0;
        // LC=4 always: the NEXTINT form carries an explicit 4-byte length, which
        // is what makes back-patching possible.
        return mu | (4 << 28) | (memberId & MAX_MEMBER_ID);
    }

    // ---- XCDR1 PL_CDR member headers -------------------------------------

    private static final int PID_EXTENDED = 0x3F01;
    private static final int MAX_SHORT_MEMBER_ID = 0x3F00;
    private static final int MAX_SHORT_LENGTH = 0xFFFF;
    private static final int MU_FLAG = 0x4000;

    /**
     * Begins a PL_CDR member: aligns to 4 and reserves the header. Returns the
     * header position for {@link #memberV1Finalize(int, int, boolean)}.
     *
     * <p>Reserves 4 bytes for an id that fits the short form, 12 otherwise. If
     * the content later overruns the short form's 16-bit length, finalize
     * promotes it.
     *
     * <p>{@code zeroFill} is what reserves the span — it is the mechanism that
     * advances {@code pos}, not just hygiene. Its zeroing is redundant on the
     * normal path, since {@link #memberV1Finalize(int, int, boolean)} always
     * overwrites the whole span; it is belt-and-braces for a caller that
     * begins a member and abandons it without finalizing.
     */
    public int memberV1Begin(int memberId) {
        requireMemberId(memberId);
        align(4);
        int headerPos = pos;
        zeroFill(memberId <= MAX_SHORT_MEMBER_ID ? 4 : 12);
        return headerPos;
    }

    /** Back-patches a PL_CDR member header, promoting to the long form if needed. */
    public void memberV1Finalize(int headerPos, int memberId, boolean mustUnderstand) {
        int flags = mustUnderstand ? MU_FLAG : 0;
        boolean shortReserved = memberId <= MAX_SHORT_MEMBER_ID;
        int contentStart = headerPos + (shortReserved ? 4 : 12);
        int contentLen = pos - contentStart;

        if (shortReserved && contentLen <= MAX_SHORT_LENGTH) {
            pooled.buffer.putShort(headerPos, (short) (flags | (memberId & 0x3FFF)));
            pooled.buffer.putShort(headerPos + 2, (short) contentLen);
            return;
        }

        if (shortReserved) {
            // Content outgrew the short form. Make room for 8 more header bytes
            // by shifting the content forward, walking backwards so overlapping
            // ranges do not clobber themselves.
            ensure(8);
            ByteBuffer b = pooled.buffer;
            int from = headerPos + 4;
            int count = pos - from;
            for (int i = count - 1; i >= 0; i--) {
                b.put(from + 8 + i, b.get(from + i));
            }
            // No need to zero [from, from + 8): the four writes below cover
            // [headerPos, headerPos + 12) unconditionally and contiguously,
            // so nothing can observe whatever was left in the vacated span.
            pos += 8;
        }

        pooled.buffer.putShort(headerPos, (short) (flags | PID_EXTENDED));
        pooled.buffer.putShort(headerPos + 2, (short) 8);
        pooled.buffer.putInt(headerPos + 4, memberId);
        pooled.buffer.putInt(headerPos + 8, pos - (headerPos + 12));
    }

    /** Writes the PL_CDR sentinel that terminates an XCDR1 mutable struct. */
    public void endMutableStruct() {
        align(4);
        writeU16(MEMBER_ID_SENTINEL);
        writeU16(0);
    }
}
