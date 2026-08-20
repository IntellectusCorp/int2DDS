package com.intellectus.int2dds.conditions;

import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * A {@link ReadCondition} additionally filtered by an SQL-like query
 * expression over the reader's cached samples.
 *
 * <p>Obtained from {@code DataReader.createQueryCondition(...)}: each call
 * mints a new native box, so this wraps its own handle rather than one
 * shared with the reader. Deleted the same way as a plain {@link
 * ReadCondition} ({@code int2dds_readcondition_delete}) — a query condition
 * is a read condition on the native side.
 */
public final class QueryCondition extends ReadCondition {
    /**
     * Wraps a query-condition handle already minted by the native layer
     * (e.g. by {@code int2dds_datareader_create_querycondition}). Meant for
     * the entity {@code createQueryCondition()} factory, not for wrapping an
     * arbitrary handle.
     */
    public QueryCondition(long rawHandle) {
        super(rawHandle);
    }

    /**
     * Sets the query's parameters (the {@code %0}, {@code %1}, ... tokens in
     * the query expression). Each entry crosses as UTF-8 {@code byte[]},
     * never {@code String}.
     */
    public void setQueryParameters(byte[][] params) {
        long h = handle();
        int rc = FfiAccess.querySetParameters(h, params, params.length);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }
}
