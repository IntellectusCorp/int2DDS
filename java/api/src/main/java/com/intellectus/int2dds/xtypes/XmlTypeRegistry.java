package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.charset.Charset;

/**
 * Loads XTypes struct descriptions from XML and hands back a {@link
 * DynamicTypeSupport} per named type, the entry point for the dynamic write
 * path. NativeCleaner-managed like {@link TypeInfo}.
 */
public final class XmlTypeRegistry implements AutoCloseable {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private final NativeHandle handle;

    public XmlTypeRegistry() {
        long[] out = new long[1];
        int rc = FfiAccess.xmlTypeRegistryCreate(out);
        ReturnCodes.check(rc);
        this.handle = NativeCleaner.register(this, out[0], XmlTypeRegistry::deleteVoid);
    }

    /** Creates a new registry pre-loaded from the XML file at {@code path}. */
    public static XmlTypeRegistry fromFile(String path) {
        long[] out = new long[1];
        int rc = FfiAccess.xmlTypeRegistryFromFile(path.getBytes(UTF8), out);
        ReturnCodes.check(rc);
        return new XmlTypeRegistry(out[0]);
    }

    /** Wraps an already-created native registry handle, e.g. from {@link #fromFile}. */
    private XmlTypeRegistry(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, XmlTypeRegistry::deleteVoid);
    }

    private static int deleteVoid(long h) {
        FfiAccess.xmlTypeRegistryDestroy(h);
        return 0;
    }

    long handle() {
        return handle.value();
    }

    /** Loads XML type descriptions into this registry, adding to what is already loaded. */
    public void loadString(String xml) {
        long h = handle();
        int rc = FfiAccess.xmlTypeRegistryLoadStr(h, xml.getBytes(UTF8));
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** Loads additional XML type descriptions from a file, adding to what is already loaded. */
    public void loadFile(String path) {
        long h = handle();
        int rc = FfiAccess.xmlTypeRegistryLoadFile(h, path.getBytes(UTF8));
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** The number of types currently loaded into this registry. */
    public int typeCount() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.xmlTypeRegistryTypeCount(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return (int) out[0];
    }

    /** Looks up a loaded type by name. */
    public DynamicTypeSupport getTypeSupport(String name) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.xmlTypeRegistryGetTypeSupport(h, name.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new DynamicTypeSupport(out[0]);
    }

    /**
     * Looks up a loaded type's top-level {@link TypeObject} by name, the
     * handle {@link com.intellectus.int2dds.core.DomainParticipant#dynamicDataFromSample}
     * decodes against.
     */
    public TypeObject getTypeObject(String name) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.xmlTypeRegistryGetTypeObject(h, name.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new TypeObject(out[0]);
    }

    /** The fully-qualified name of the type at {@code index} (0-based, order loaded). */
    public String typeName(int index) {
        long h = handle();
        byte[][] out = new byte[1][];
        int rc = FfiAccess.xmlTypeRegistryTypeName(h, index, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new String(out[0], UTF8);
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
