package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import java.util.Objects;

/**
 * Reads a single FLAT field straight out of a serialized sample's raw CDR
 * bytes (e.g. from {@link com.intellectus.int2dds.core.DataReader#takeSerialized})
 * against a {@link TypeObject}, without materializing a full {@link
 * DynamicData} -- a lightweight primitive for per-field routing/filtering
 * (a gateway or router deciding whether to forward a sample based on one
 * key field).
 *
 * <p>FLAT types only, the same restriction as the CDR-field-descriptor
 * content-filter path: a nested struct, sequence, array or map field, or a
 * type that is not itself flat, throws rather than decoding. An unknown
 * field name throws too. Each call here re-decodes the whole sample from
 * scratch, so a caller reading several fields off the same sample should
 * build one {@link DynamicData} instead (see {@link
 * com.intellectus.int2dds.core.DomainParticipant#dynamicDataFromSample}) and
 * read every field off that.
 */
public final class DynamicSample {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private DynamicSample() {}

    /** Reads a {@code bool} field named {@code field}. Throws if the field is missing or not a bool. */
    public static boolean getBool(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(1).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetBool(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.get(0) != 0;
    }

    /** Reads an {@code int8} field named {@code field}. Throws if the field is missing or not an int8. */
    public static byte getI8(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(1).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetI8(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.get(0);
    }

    /** Reads a {@code uint8} field named {@code field}, its unsigned 0-255 value. Throws if missing or not a uint8. */
    public static int getU8(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(1).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetU8(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.get(0) & 0xFF;
    }

    /** Reads a {@code byte} (octet) field named {@code field}. Throws if the field is missing or not a byte. */
    public static byte getByte(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(1).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetByte(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.get(0);
    }

    /** Reads an {@code int16} field named {@code field}. Throws if the field is missing or not an int16. */
    public static short getI16(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(2).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetI16(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.getShort(0);
    }

    /** Reads a {@code uint16} field named {@code field}, its unsigned 0-65535 value. Throws if missing or not a uint16. */
    public static int getU16(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(2).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetU16(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.getShort(0) & 0xFFFF;
    }

    /** Reads an {@code int32} field named {@code field}. Throws if the field is missing or not an int32. */
    public static int getI32(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetI32(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.getInt(0);
    }

    /**
     * Reads a {@code uint32} field named {@code field}, its unsigned magnitude
     * widened into a {@code long}. Throws if the field is missing or not a
     * uint32.
     */
    public static long getU32(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetU32(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.getInt(0) & 0xFFFFFFFFL;
    }

    /** Reads an {@code int64} field named {@code field}. Throws if the field is missing or not an int64. */
    public static long getI64(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetI64(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.getLong(0);
    }

    /**
     * Reads a {@code uint64} field named {@code field}, its raw 64 bits as a
     * {@code long} -- a value at or above 2^63 reads back negative. Throws if
     * the field is missing or not a uint64.
     */
    public static long getU64(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetU64(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.getLong(0);
    }

    /** Reads a {@code float32} field named {@code field}. Throws if the field is missing or not a float32. */
    public static float getF32(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetF32(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.getFloat(0);
    }

    /** Reads a {@code float64} field named {@code field}. Throws if the field is missing or not a float64. */
    public static double getF64(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetF64(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return out.getDouble(0);
    }

    /**
     * Reads a {@code char8} field named {@code field} as its unsigned 0-255
     * byte value. Throws if the field is missing or not a char8.
     */
    public static char getChar8(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        ByteBuffer out = ByteBuffer.allocateDirect(1).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.dynamicSampleGetChar8(FfiAccess.directBufferAddress(cdrBuf), cdr.length,
                type.handle(), field.getBytes(UTF8), FfiAccess.directBufferAddress(out));
        keepAlive(cdrBuf, out, type);
        ReturnCodes.check(rc);
        return (char) (out.get(0) & 0xFF);
    }

    /** Reads a {@code string}/{@code wstring} field named {@code field}. Throws if the field is missing or not one. */
    public static String getString(byte[] cdr, TypeObject type, String field) {
        Objects.requireNonNull(cdr, "cdr");
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(field, "field");
        ByteBuffer cdrBuf = directCopy(cdr);
        long cdrAddr = FfiAccess.directBufferAddress(cdrBuf);
        long typeHandle = type.handle();
        byte[] fieldBytes = field.getBytes(UTF8);

        int cap = 64;
        while (true) {
            byte[] buf = new byte[cap];
            ByteBuffer outLenSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = FfiAccess.dynamicSampleGetString(cdrAddr, cdr.length, typeHandle, fieldBytes,
                    buf, cap, FfiAccess.directBufferAddress(outLenSlot));
            keepAlive(cdrBuf, outLenSlot, type);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                cap = (int) outLenSlot.getLong(0) + 1; // out_len excludes the NUL
                continue;
            }
            ReturnCodes.check(rc);
            int n = (int) outLenSlot.getLong(0); // excludes the NUL -- no -1 needed
            return new String(buf, 0, n, UTF8);
        }
    }

    private static ByteBuffer directCopy(byte[] cdr) {
        ByteBuffer buf = ByteBuffer.allocateDirect(cdr.length).order(ByteOrder.nativeOrder());
        buf.put(cdr);
        buf.rewind();
        return buf;
    }

    private static void keepAlive(ByteBuffer cdrBuf, ByteBuffer out, TypeObject type) {
        NativeKeepAlive.keepAlive(cdrBuf);
        NativeKeepAlive.keepAlive(out);
        NativeKeepAlive.keepAlive(type);
    }
}
