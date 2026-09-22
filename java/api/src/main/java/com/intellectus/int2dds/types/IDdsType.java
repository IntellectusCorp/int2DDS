package com.intellectus.int2dds.types;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.xtypes.TypeInfo;

/**
 * A type that can cross the DDS wire.
 *
 * <p>Deliberately not the C# shape. The C# binding's {@code IDdsType} returns
 * {@code byte[]} from {@code SerializeCdr()}, which works there because
 * {@code fixed} can pin a managed array and hand its address straight to the
 * C ABI. Java has no {@code fixed}: a returned array would have to be allocated
 * per sample and then copied into a direct buffer, giving back the zero-copy
 * path the CDR layer was built to provide.
 *
 * <p>So the caller supplies a pooled {@link CdrWriter} and the type writes into
 * it — no allocation, no copy. The IDL backend generates implementations of
 * this interface.
 */
public interface IDdsType {

    /** Writes this instance into {@code writer} in CDR form. */
    void serializeCdr(CdrWriter writer);

    /** Replaces this instance's fields with those decoded from {@code reader}. */
    void deserializeCdr(CdrReader reader);

    /**
     * The DDS type name used when creating a topic. The core matches publishers
     * to subscribers on it, so it must be stable and must not be derived from
     * the JVM class name.
     */
    String typeName();

    /**
     * The extensibility this type encodes with. {@code int2dds_create_topic}
     * takes it as an argument, and it must agree with what
     * {@link #serializeCdr} actually emits — an APPENDABLE type writes a
     * struct-level DHEADER, a FINAL one does not.
     */
    Extensibility extensibility();

    /**
     * A fresh description of every member of this type -- kinds, bounds and
     * {@code @key} flags -- or {@code null} when the type carries none. With
     * one, {@code createTopic} advertises the full TypeObject over discovery
     * and the core resolves instance keys, which is what lets a keyed type
     * match a keyed peer written in another language. Without one the topic is
     * key-less and matched by type name alone. The caller closes the result.
     * The IDL backend generates this.
     */
    default TypeInfo typeInfo() {
        return null;
    }
}

// The C# binding keeps these three facts in a DdsTypeAttribute
// (csharp/src/Int2Dds/Types/IDdsType.cs) rather than on the interface. Java
// uses interface methods because an annotation costs a reflection lookup at
// every use, and the type name is read on every topic creation.
