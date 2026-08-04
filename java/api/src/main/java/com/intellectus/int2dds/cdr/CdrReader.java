package com.intellectus.int2dds.cdr;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/**
 * CDR decoder over a caller-owned {@link ByteBuffer}.
 *
 * <p>Does not copy and does not own the buffer. The receive path wraps a native
 * sample loan in a direct buffer and hands it here; tests hand it a heap buffer.
 * The caller keeps the buffer valid for the reader's lifetime — for a loan, that
 * means until the loan is returned.
 */
public final class CdrReader {

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

    private final ByteBuffer data;
    private final int limit;
    private final boolean xcdr2;
    private final int headerSize;
    private int pos;

    private CdrReader(ByteBuffer data, boolean littleEndian, boolean xcdr2, int headerSize) {
        this.data = data.slice();
        this.data.order(littleEndian ? ByteOrder.LITTLE_ENDIAN : ByteOrder.BIG_ENDIAN);
        this.limit = this.data.remaining();
        this.xcdr2 = xcdr2;
        this.headerSize = headerSize;
        this.pos = headerSize;
    }

    /** Parses the 4-byte encapsulation header to pick endianness and version. */
    public static CdrReader of(ByteBuffer data) {
        ByteBuffer slice = data.slice();
        if (slice.remaining() < 4) {
            throw new CdrUnderflowException("Data too short for encapsulation header.");
        }
        // The encapsulation id is always big-endian, whatever follows it is not.
        // Read from indices 0 and 1 of the slice, where 0 is the window's start.
        int encapId = ((slice.get(0) & 0xFF) << 8) | (slice.get(1) & 0xFF);

        boolean littleEndian;
        boolean xcdr2;
        switch (encapId) {
            case ENCAP_CDR_LE:
            case ENCAP_PL_CDR_LE:
                littleEndian = true;
                xcdr2 = false;
                break;
            case ENCAP_CDR_BE:
            case ENCAP_PL_CDR_BE:
                littleEndian = false;
                xcdr2 = false;
                break;
            case ENCAP_CDR2_LE:
            case ENCAP_D_CDR2_LE:
            case ENCAP_PL_CDR2_LE:
                littleEndian = true;
                xcdr2 = true;
                break;
            case ENCAP_CDR2_BE:
            case ENCAP_D_CDR2_BE:
            case ENCAP_PL_CDR2_BE:
                littleEndian = false;
                xcdr2 = true;
                break;
            default:
                throw new CdrInvalidEncapsulationException(
                        "Unrecognized encapsulation ID: 0x"
                                + String.format("%04X", encapId));
        }
        return new CdrReader(slice, littleEndian, xcdr2, 4);
    }

    /** Convenience for a {@code byte[]} payload. */
    public static CdrReader of(byte[] data) {
        return of(ByteBuffer.wrap(data));
    }

    /** A reader over data with no encapsulation header, e.g. a serialized key. */
    public static CdrReader ofRaw(ByteBuffer data, boolean littleEndian, boolean xcdr2) {
        return new CdrReader(data, littleEndian, xcdr2, 0);
    }

    public int remaining() {
        return Math.max(0, limit - pos);
    }

    public int position() {
        return pos;
    }

    public boolean isXcdr2() {
        return xcdr2;
    }

    /** Advances without decoding. */
    public void skip(int count) {
        require(count);
        pos += count;
    }

    // ---- alignment and guards -------------------------------------------

    private void align(int alignment) {
        if (alignment <= 1) {
            return;
        }
        int actual = xcdr2 ? Math.min(alignment, 4) : alignment;
        int streamPos = pos - headerSize;
        int aligned = (streamPos + actual - 1) & ~(actual - 1);
        int newPos = aligned + headerSize;
        if (newPos > limit) {
            throw new CdrUnderflowException("Alignment would exceed buffer.");
        }
        pos = newPos;
    }

    private void require(int count) {
        if (pos + count > limit) {
            throw new CdrUnderflowException(
                    "Need " + count + " bytes but only " + remaining() + " remaining.");
        }
    }

    // ---- primitives -----------------------------------------------------

    public boolean readBool() {
        return readI8() != 0;
    }

    public byte readI8() {
        require(1);
        byte v = data.get(pos);
        pos += 1;
        return v;
    }

    /** The byte widened to its unsigned value, 0..255. */
    public int readU8() {
        return readI8() & 0xFF;
    }

    public short readI16() {
        align(2);
        require(2);
        short v = data.getShort(pos);
        pos += 2;
        return v;
    }

    /** The short widened to its unsigned value, 0..65535. */
    public int readU16() {
        return readI16() & 0xFFFF;
    }

    public int readI32() {
        align(4);
        require(4);
        int v = data.getInt(pos);
        pos += 4;
        return v;
    }

    /** The raw 32 bits. Use {@code Integer.toUnsignedLong} for the numeric value. */
    public int readU32() {
        return readI32();
    }

    public long readI64() {
        align(8);
        require(8);
        long v = data.getLong(pos);
        pos += 8;
        return v;
    }

    /** The raw 64 bits. Use {@code Long.toUnsignedString} for the numeric value. */
    public long readU64() {
        return readI64();
    }

    public float readF32() {
        return Float.intBitsToFloat(readI32());
    }

    public double readF64() {
        return Double.longBitsToDouble(readI64());
    }

    // ---- strings, sequences, raw bytes -----------------------------------

    private static final java.nio.charset.Charset UTF8 =
            java.nio.charset.Charset.forName("UTF-8");

    /**
     * Reads a CDR string: a uint32 length including the terminator, then that
     * many bytes, of which the trailing NUL is dropped.
     */
    public String readString() {
        int lenWithNul = readI32();
        if (lenWithNul <= 0) {
            throw new CdrUnderflowException("Invalid string length: " + lenWithNul);
        }
        require(lenWithNul);
        int contentLen = lenWithNul - 1;
        byte[] bytes = new byte[contentLen];
        for (int i = 0; i < contentLen; i++) {
            bytes[i] = data.get(pos + i);
        }
        pos += lenWithNul;   // includes the terminator
        return new String(bytes, UTF8);
    }

    /**
     * Reads a CDR wstring: a uint32 count of UTF-16 units including the
     * terminator, then that many u16 units, of which the trailing zero is dropped.
     */
    public String readWString() {
        int unitsWithNul = readI32();
        if (unitsWithNul <= 0) {
            throw new CdrUnderflowException("Invalid wstring length: " + unitsWithNul);
        }
        int units = unitsWithNul - 1;
        char[] chars = new char[units];
        for (int i = 0; i < units; i++) {
            chars[i] = (char) readU16();
        }
        readU16();   // terminator
        return new String(chars);
    }

    /** Reads a sequence header and returns the element count. */
    public int readSeqHeader() {
        return readI32();
    }

    /** Reads raw bytes with no alignment. */
    public byte[] readBytes(int length) {
        require(length);
        byte[] out = new byte[length];
        for (int i = 0; i < length; i++) {
            out[i] = data.get(pos + i);
        }
        pos += length;
        return out;
    }

    /** Reads an enum discriminant. */
    public int readEnum() {
        return readI32();
    }
}
