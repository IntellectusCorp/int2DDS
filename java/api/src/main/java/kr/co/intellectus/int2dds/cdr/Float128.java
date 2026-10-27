package kr.co.intellectus.int2dds.cdr;

import java.nio.charset.Charset;
import kr.co.intellectus.int2dds.internal.ReturnCodes;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * IDL {@code float128} ({@code long double}) value.
 *
 * <p>Java has no binary128 type, so the value is carried as its raw IEEE 754 bits; going through
 * {@code double} would drop the low 60 mantissa bits that peers compare. Equality is bitwise.
 */
public final class Float128 {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    public static final Float128 ZERO = new Float128(0L, 0L);

    private final long lo;
    private final long hi;

    /** From the low and high 64 bits of the binary128 encoding. */
    public Float128(long lo, long hi) {
        this.lo = lo;
        this.hi = hi;
    }

    public long lo() {
        return lo;
    }

    public long hi() {
        return hi;
    }

    /** Decimal literal correctly rounded to binary128 (by the native core parser). */
    public static Float128 parse(String text) {
        byte[] out = new byte[16];
        ReturnCodes.check(FfiAccess.float128Parse(text.getBytes(UTF8), out));
        return fromBytes(out, 0);
    }

    /** Exact widening of a double. */
    public static Float128 fromDouble(double value) {
        byte[] out = new byte[16];
        ReturnCodes.check(FfiAccess.float128FromF64(value, out));
        return fromBytes(out, 0);
    }

    /** Nearest double. */
    public double toDouble() {
        double[] out = new double[1];
        ReturnCodes.check(FfiAccess.float128ToF64(toBytes(), out));
        return out[0];
    }

    /** 16 bytes, little-endian. */
    public byte[] toBytes() {
        byte[] b = new byte[16];
        for (int i = 0; i < 8; i++) {
            b[i] = (byte) (lo >>> (8 * i));
            b[8 + i] = (byte) (hi >>> (8 * i));
        }
        return b;
    }

    /** Reads 16 little-endian bytes starting at {@code offset}. */
    public static Float128 fromBytes(byte[] b, int offset) {
        long lo = 0;
        long hi = 0;
        for (int i = 7; i >= 0; i--) {
            lo = (lo << 8) | (b[offset + i] & 0xFFL);
            hi = (hi << 8) | (b[offset + 8 + i] & 0xFFL);
        }
        return new Float128(lo, hi);
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Float128 && ((Float128) o).lo == lo && ((Float128) o).hi == hi;
    }

    @Override
    public int hashCode() {
        return Long.hashCode(lo) * 31 + Long.hashCode(hi);
    }

    @Override
    public String toString() {
        return toDouble() + String.format(" (0x%016x%016x)", hi, lo);
    }
}
