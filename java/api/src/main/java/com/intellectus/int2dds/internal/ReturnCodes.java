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
import com.intellectus.int2dds.exceptions.ReturnCodeValues;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.charset.Charset;

/**
 * Turns {@code Int2DdsRet} values into exceptions.
 *
 * <p>The numeric values switched on below are {@link ReturnCodeValues}, which
 * is transcribed from the {@code INT2DDS_RET_*} constants in {@code
 * ffi/src/error.rs} — the single source of truth for this mapping, and the
 * only place the numbers are written down. If a code is added there, add a
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
        if (ret == ReturnCodeValues.OK) {
            return;
        }
        throw toException(ret);
    }

    /** True for OK, false for NO_DATA, throws for anything else.
     *  Read and take need to report "nothing available" without an exception. */
    public static boolean checkOrNoData(int ret) {
        if (ret == ReturnCodeValues.OK) {
            return true;
        }
        if (ret == ReturnCodeValues.NO_DATA) {
            return false;
        }
        throw toException(ret);
    }

    /**
     * The native layer's last error message for this thread, or an empty string.
     *
     * <p>The FFI returns the message's full length even when the buffer was too
     * small, so a single retry with the exact size always succeeds.
     */
    public static String lastErrorMessage() {
        byte[] buf = new byte[FIRST_TRY_BYTES];
        int len = FfiAccess.lastErrorMessage(buf);
        if (len <= 0) {
            return "";
        }
        if (len > buf.length) {
            buf = new byte[len];
            len = FfiAccess.lastErrorMessage(buf);
            if (len <= 0) {
                return "";
            }
            len = Math.min(len, buf.length);
        }
        return new String(buf, 0, len, UTF8);
    }

    private static DdsException toException(int ret) {
        switch (ret) {
            case ReturnCodeValues.ERROR:
                return new DdsErrorException(lastErrorMessage());
            case ReturnCodeValues.TIMEOUT:
                return new DdsTimeoutException();
            case ReturnCodeValues.UNSUPPORTED:
                return new DdsUnsupportedException();
            case ReturnCodeValues.INVALID_ARGUMENT:
                return new DdsInvalidArgumentException();
            case ReturnCodeValues.ALREADY_DELETED:
                return new DdsAlreadyDeletedException();
            case ReturnCodeValues.NOT_ENABLED:
                return new DdsNotEnabledException();
            case ReturnCodeValues.IMMUTABLE_POLICY:
                return new DdsImmutablePolicyException();
            case ReturnCodeValues.INCONSISTENT_POLICY:
                return new DdsInconsistentPolicyException();
            case ReturnCodeValues.PRECONDITION_NOT_MET:
                return new DdsPreconditionNotMetException();
            case ReturnCodeValues.OUT_OF_RESOURCES:
                return new DdsOutOfResourcesException();
            case ReturnCodeValues.ILLEGAL_OPERATION:
                return new DdsIllegalOperationException();
            case ReturnCodeValues.NO_DATA:
                return new DdsNoDataException();
            case ReturnCodeValues.NULL_POINTER:
                return new DdsNullPointerException();
            case ReturnCodeValues.BUFFER_TOO_SMALL:
                return new DdsBufferTooSmallException();
            default:
                return new DdsException(
                        "DDS operation failed with unknown code " + ret + ".", ret);
        }
    }
}
