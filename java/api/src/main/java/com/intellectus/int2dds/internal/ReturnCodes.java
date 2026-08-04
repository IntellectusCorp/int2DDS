package com.intellectus.int2dds.internal;

import com.intellectus.int2dds.exceptions.DdsAlreadyDeletedException;
import com.intellectus.int2dds.exceptions.DdsBufferTooSmallException;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.exceptions.DdsIllegalOperationException;
import com.intellectus.int2dds.exceptions.DdsImmutablePolicyException;
import com.intellectus.int2dds.exceptions.DdsInconsistentPolicyException;
import com.intellectus.int2dds.exceptions.DdsInvalidArgumentException;
import com.intellectus.int2dds.exceptions.DdsNoDataException;
import com.intellectus.int2dds.exceptions.DdsNotEnabledException;
import com.intellectus.int2dds.exceptions.DdsNullPointerException;
import com.intellectus.int2dds.exceptions.DdsOutOfResourcesException;
import com.intellectus.int2dds.exceptions.DdsPreconditionNotMetException;
import com.intellectus.int2dds.exceptions.DdsTimeoutException;
import com.intellectus.int2dds.exceptions.DdsUnsupportedException;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.charset.Charset;
import java.util.function.ToIntFunction;

/**
 * Turns {@code Int2DdsRet} values into exceptions.
 *
 * <p>The numeric values switched on below are {@link DdsException}'s
 * {@code RET_*} constants, transcribed from the {@code INT2DDS_RET_*}
 * constants in {@code ffi/src/error.rs} — the single source of truth for
 * this mapping, and the only place the numbers are written down. If a code is added there, add a
 * case here — the fallback keeps working but loses the specific type. A
 * handful of codes in that file (the {@code INT2DDS_RET_DYNAMIC_*} family,
 * 200-204, for dynamic-type reflection) have no dedicated exception type yet
 * and intentionally fall through to the {@code default} case, which still
 * reports the real code.
 */
public final class ReturnCodes {

    private static final Charset UTF8 = Charset.forName("UTF-8");
    private static final int FIRST_TRY_BYTES = 256;

    private ReturnCodes() {}

    /** Throws the exception matching {@code ret}, or returns for OK. */
    public static void check(int ret) {
        if (ret == DdsException.RET_OK) {
            return;
        }
        throw toException(ret);
    }

    /** True for OK, false for NO_DATA, throws for anything else.
     *  Read and take need to report "nothing available" without an exception. */
    public static boolean checkOrNoData(int ret) {
        if (ret == DdsException.RET_OK) {
            return true;
        }
        if (ret == DdsException.RET_NO_DATA) {
            return false;
        }
        throw toException(ret);
    }

    /**
     * The native layer's last error message for this thread, or an empty string.
     *
     * <p>The FFI ({@code ffi/src/last_error.rs}) reserves the buffer's last byte
     * for a NUL terminator, so an N-byte buffer only ever receives N-1 message
     * bytes, and it always returns the message's full pre-truncation length. A
     * retry therefore needs a buffer one byte larger than that full length, not
     * equal to it.
     */
    public static String lastErrorMessage() {
        return readMessage(FfiAccess::lastErrorMessage);
    }

    /**
     * Buffer-growth logic behind {@link #lastErrorMessage()}, factored out so it
     * can be tested against a stub that mimics the native contract without a
     * message long enough to trip it actually existing in the FFI today.
     *
     * @param reader mirrors {@code FfiAccess.lastErrorMessage(byte[])}: writes as
     *     much of the message plus a NUL as fits, always returns the message's
     *     full pre-truncation byte length
     */
    static String readMessage(ToIntFunction<byte[]> reader) {
        byte[] buf = new byte[FIRST_TRY_BYTES];
        int len = reader.applyAsInt(buf);
        if (len <= 0) {
            return "";
        }
        // buf holds at most buf.length - 1 message bytes (the last byte is
        // reserved for NUL); len >= buf.length means the message did not fit,
        // including the case where it fit the reserved capacity exactly.
        if (len >= buf.length) {
            int fullLen = len;
            buf = new byte[fullLen + 1];
            len = reader.applyAsInt(buf);
            if (len <= 0) {
                return "";
            }
            len = Math.min(len, buf.length - 1);
        }
        return new String(buf, 0, len, UTF8);
    }

    private static DdsException toException(int ret) {
        switch (ret) {
            case DdsException.RET_ERROR:
                return new DdsErrorException(lastErrorMessage());
            case DdsException.RET_TIMEOUT:
                return new DdsTimeoutException();
            case DdsException.RET_UNSUPPORTED:
                return new DdsUnsupportedException();
            case DdsException.RET_INVALID_ARGUMENT:
                return new DdsInvalidArgumentException();
            case DdsException.RET_ALREADY_DELETED:
                return new DdsAlreadyDeletedException();
            case DdsException.RET_NOT_ENABLED:
                return new DdsNotEnabledException();
            case DdsException.RET_IMMUTABLE_POLICY:
                return new DdsImmutablePolicyException();
            case DdsException.RET_INCONSISTENT_POLICY:
                return new DdsInconsistentPolicyException();
            case DdsException.RET_PRECONDITION_NOT_MET:
                return new DdsPreconditionNotMetException();
            case DdsException.RET_OUT_OF_RESOURCES:
                return new DdsOutOfResourcesException();
            case DdsException.RET_ILLEGAL_OPERATION:
                return new DdsIllegalOperationException();
            case DdsException.RET_NO_DATA:
                return new DdsNoDataException();
            case DdsException.RET_NULL_POINTER:
                return new DdsNullPointerException();
            case DdsException.RET_BUFFER_TOO_SMALL:
                return new DdsBufferTooSmallException();
            default:
                return new DdsException(
                        "DDS operation failed with unknown code " + ret + ".", ret);
        }
    }
}
