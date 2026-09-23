package com.intellectus.int2dds.core;

import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.ByteBuffer;

/**
 * A DDS-owned write buffer prepared by {@link DataWriter#prepareSerializedWrite},
 * for a zero-copy transactional serialized write: write the sample's CDR
 * bytes directly into {@link #buffer()}, then either {@link #commit()} to
 * publish them or {@link #close()} (or let a try-with-resources close it
 * without committing) to abort.
 *
 * <p><b>The buffer is backed by DDS-owned memory valid only until this
 * object's commit or close.</b> Retaining {@link #buffer()}'s reference past
 * that point and reading or writing through it is undefined behavior -- a
 * use-after-free of native memory. Always use this class in a
 * try-with-resources block and never let the buffer escape it.
 *
 * <p>A strict three-state machine: {@code OPEN} on construction, then exactly
 * one of {@code COMMITTED} (via {@link #commit()}) or {@code ABORTED} (via
 * {@link #close()}, including the failure path of a failed {@link
 * #commit()}). Every method past the first commit/close throws or no-ops
 * rather than touching the freed loan again.
 */
public final class SerializedWriteBuffer implements AutoCloseable {

    private enum State {
        OPEN,
        COMMITTED,
        ABORTED
    }

    private final DataWriter<?> writer;
    private final long loan;
    private ByteBuffer buffer;
    private State state = State.OPEN;

    SerializedWriteBuffer(DataWriter<?> writer, long loan, ByteBuffer buffer) {
        this.writer = writer;
        this.loan = loan;
        this.buffer = buffer;
    }

    /**
     * The writable direct buffer backing this loan. Write the sample's CDR
     * bytes (encapsulation header included) starting at its current
     * position; {@link #commit()} publishes exactly {@code buffer().position()}
     * bytes.
     *
     * @throws IllegalStateException if this buffer is not {@code OPEN}
     *     (already committed or aborted/closed)
     */
    public ByteBuffer buffer() {
        if (state != State.OPEN) {
            throw new IllegalStateException(
                    "serialized write buffer is no longer open (state=" + state + ")");
        }
        return buffer;
    }

    /**
     * Publishes {@link #buffer()}'s current position as the sample's byte
     * length. On success the underlying loan is consumed and freed natively;
     * on failure the loan is aborted here (freed exactly once) before the
     * mapped return-code exception is thrown.
     *
     * @throws IllegalStateException if this buffer is not {@code OPEN}
     * @throws com.intellectus.int2dds.exceptions.DdsException if the native
     *     commit fails
     */
    public void commit() {
        if (state != State.OPEN) {
            throw new IllegalStateException(
                    "serialized write buffer is no longer open (state=" + state + ")");
        }
        int rc = FfiAccess.datawriterCommitSerializedWrite(
                writer.handle(), loan, buffer.position());
        NativeKeepAlive.keepAlive(writer);
        if (rc == 0) {
            state = State.COMMITTED;
            buffer = null;
            return;
        }
        // The loan is still valid on any non-OK code and must be freed
        // exactly once -- abort it here before surfacing the failure.
        FfiAccess.datawriterAbortSerializedWrite(loan);
        state = State.ABORTED;
        buffer = null;
        ReturnCodes.check(rc);
    }

    /**
     * Aborts this loan if it is still {@code OPEN}, freeing the DDS-owned
     * buffer without publishing anything. A no-op once {@code COMMITTED} or
     * {@code ABORTED} -- idempotent, safe to call more than once and safe as
     * the implicit close of a try-with-resources after an explicit {@link
     * #commit()}.
     */
    @Override
    public void close() {
        if (state != State.OPEN) {
            return;
        }
        FfiAccess.datawriterAbortSerializedWrite(loan);
        state = State.ABORTED;
        buffer = null;
    }
}
