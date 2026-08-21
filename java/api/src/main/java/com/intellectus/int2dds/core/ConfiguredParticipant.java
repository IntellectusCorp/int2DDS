package com.intellectus.int2dds.core;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.xtypes.DynamicDataReader;
import com.intellectus.int2dds.xtypes.DynamicDataWriter;
import java.nio.charset.Charset;
import java.util.HashMap;
import java.util.Map;
import java.util.Objects;

/**
 * A whole participant tree — participant, publishers/subscribers,
 * datawriters/datareaders and topics — built declaratively from a loaded XML
 * {@code domain_participant_library} entry by {@link
 * DomainParticipantFactory#createParticipantFromConfig}.
 *
 * <p>The tree's structure is not otherwise exposed: its datawriters and
 * datareaders are fetched by name ({@code "publisher::writer"}, {@code
 * "subscriber::reader"}) via {@link #getDataWriter} / {@link #getDataReader}.
 * Each fetched wrapper is an owning {@link DynamicDataWriter} or {@link
 * DynamicDataReader}, cached by name so repeated lookups of the same name
 * return the same instance rather than a fresh wrapper each time.
 *
 * <p><b>Teardown order matters.</b> The native writer/reader handles this
 * class hands out are Arc-clones of the tree's own copies, sharing one atomic
 * "deleted" flag per endpoint: closing either clone unregisters the endpoint
 * from its parent publisher/subscriber, and the shared flag makes a second
 * unregister on the other clone a safe no-op. {@link #close()} relies on
 * that: it closes every cached fetched writer/reader first — unregistering
 * them while the tree's publishers/subscribers are still alive — and only
 * then releases the tree itself. Not thread-safe, the same as every other
 * entity in this binding — see {@link NativeHandle#value()}'s own doc.
 */
public final class ConfiguredParticipant implements AutoCloseable {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private final NativeHandle handle;
    private final Map<String, DynamicDataWriter> writers = new HashMap<>();
    private final Map<String, DynamicDataReader> readers = new HashMap<>();

    ConfiguredParticipant(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, ConfiguredParticipant::deleteVoid);
    }

    private static int deleteVoid(long h) {
        FfiAccess.configuredParticipantDestroy(h);
        return 0;
    }

    long handle() {
        return handle.value();
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    /**
     * The datawriter named {@code "publisher::writer"} in this tree, cached
     * so repeated calls with the same name return the same instance.
     *
     * @throws NullPointerException if {@code name} is null
     * @throws com.intellectus.int2dds.exceptions.DdsException if no
     *     datawriter with that name exists in the tree
     */
    public DynamicDataWriter getDataWriter(String name) {
        Objects.requireNonNull(name, "name");
        DynamicDataWriter cached = writers.get(name);
        if (cached != null) {
            return cached;
        }
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.configuredParticipantGetDataWriter(h, name.getBytes(UTF8), out);
        // h was read moments before the native call that consumes it; keep
        // this reachable across it -- see NativeKeepAlive's own doc.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        DynamicDataWriter created = DynamicDataWriter.fromHandle(out[0]);
        writers.put(name, created);
        return created;
    }

    /**
     * The datareader named {@code "subscriber::reader"} in this tree, cached
     * so repeated calls with the same name return the same instance.
     *
     * @throws NullPointerException if {@code name} is null
     * @throws com.intellectus.int2dds.exceptions.DdsException if no
     *     datareader with that name exists in the tree
     */
    public DynamicDataReader getDataReader(String name) {
        Objects.requireNonNull(name, "name");
        DynamicDataReader cached = readers.get(name);
        if (cached != null) {
            return cached;
        }
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.configuredParticipantGetDataReader(h, name.getBytes(UTF8), out);
        // Same fence as getDataWriter.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        DynamicDataReader created = DynamicDataReader.fromHandle(out[0]);
        readers.put(name, created);
        return created;
    }

    /**
     * Closes every cached fetched writer/reader (unregistering each from its
     * still-live parent publisher/subscriber), then releases the whole tree.
     * See the class javadoc for why that order is the safe one. Idempotent:
     * a second call finds nothing left to close and releases nothing again.
     */
    @Override
    public void close() {
        for (DynamicDataWriter w : writers.values()) {
            w.close();
        }
        writers.clear();
        for (DynamicDataReader r : readers.values()) {
            r.close();
        }
        readers.clear();
        ReturnCodes.check(handle.close());
    }
}
