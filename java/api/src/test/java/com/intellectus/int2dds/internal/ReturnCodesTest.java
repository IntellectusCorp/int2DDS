package com.intellectus.int2dds.internal;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.exceptions.DdsAlreadyDeletedException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.exceptions.DdsNoDataException;
import com.intellectus.int2dds.exceptions.DdsTimeoutException;
import org.junit.jupiter.api.Test;

class ReturnCodesTest {

    // Values from ffi/src/error.rs (INT2DDS_RET_* constants), the single
    // source of truth for this mapping.
    private static final int RET_TIMEOUT = 2;
    private static final int RET_NO_DATA = 27;
    private static final int RET_ALREADY_DELETED = 20;
    private static final int[] KNOWN_CODES = {
        1, // ERROR
        2, // TIMEOUT
        3, // UNSUPPORTED
        11, // INVALID_ARGUMENT
        20, // ALREADY_DELETED
        21, // NOT_ENABLED
        22, // IMMUTABLE_POLICY
        23, // INCONSISTENT_POLICY
        24, // PRECONDITION_NOT_MET
        25, // OUT_OF_RESOURCES
        26, // ILLEGAL_OPERATION
        27, // NO_DATA
        100, // NULL_POINTER
        101, // BUFFER_TOO_SMALL
    };

    @Test
    void okDoesNotThrow() {
        assertDoesNotThrow(() -> ReturnCodes.check(0));
    }

    @Test
    void timeoutMapsToItsOwnType() {
        DdsTimeoutException e =
                assertThrows(DdsTimeoutException.class, () -> ReturnCodes.check(RET_TIMEOUT));
        assertEquals(RET_TIMEOUT, e.getCode());
    }

    @Test
    void alreadyDeletedMapsToItsOwnType() {
        assertThrows(DdsAlreadyDeletedException.class,
                () -> ReturnCodes.check(RET_ALREADY_DELETED));
    }

    @Test
    void everyKnownCodeMapsToADistinctType() {
        // A switch that falls through to the generic type for a code it should
        // handle is invisible at a call site — the caller still sees a
        // DdsException. Assert the concrete type for all of them at once.
        java.util.Set<Class<?>> seen = new java.util.HashSet<>();
        for (int code : KNOWN_CODES) {
            DdsException e = assertThrows(DdsException.class, () -> ReturnCodes.check(code));
            assertEquals(code, e.getCode(), "code round-trips onto the exception");
            assertNotEquals(DdsException.class, e.getClass(),
                    "code " + code + " fell through to the generic type");
            assertTrue(seen.add(e.getClass()), "two codes share one type: " + e.getClass());
        }
    }

    @Test
    void unknownCodeFallsBackToTheBaseTypeAndKeepsTheCode() {
        DdsException e = assertThrows(DdsException.class, () -> ReturnCodes.check(9999));
        assertEquals(DdsException.class, e.getClass());
        assertEquals(9999, e.getCode());
        assertTrue(e.getMessage().contains("9999"), e.getMessage());
    }

    @Test
    void noDataIsReportedWithoutThrowing() {
        assertTrue(ReturnCodes.checkOrNoData(0));
        assertFalse(ReturnCodes.checkOrNoData(RET_NO_DATA));
        assertThrows(DdsTimeoutException.class, () -> ReturnCodes.checkOrNoData(RET_TIMEOUT));
    }

    @Test
    void lastErrorMessageReadsBackThroughJni() {
        // Exercises the *mut c_char write-back fixed in 1d27f9f5. Before that
        // fix this returned "" no matter what, and a length-only assertion
        // would not have noticed.
        String msg = ReturnCodes.lastErrorMessage();
        assertNotNull(msg, "must return a string, empty when no error is pending");
    }
}
