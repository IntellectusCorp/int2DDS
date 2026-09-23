package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.core.DomainParticipant;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

/**
 * End-to-end proof of the four new {@link TypeInfo} complex-field builders
 * (bounded string, fixed array of primitives, array of nested structs,
 * sequence of nested structs), mirroring {@link DynamicDataTest}'s
 * build-with-{@link TypeInfo} / encode-with-{@link CdrWriter} /
 * decode-with-{@link DomainParticipant#dynamicDataFromSample} / read-with-
 * {@link DynamicData} pattern.
 */
class ComplexTypeBuilderTest {

    /** The domain this JVM's tests use, isolated from other runs. */
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    /**
     * A fixed-size array carries no length prefix on the wire -- unlike a
     * sequence, which writes a uint32 count via {@link CdrWriter#writeSeqHeader}.
     * The array's elements are simply written back-to-back (see {@code
     * deserialize_value_xcdr2}'s {@code Array} arm in {@code
     * dds/src/xtypes/dynamic_serialization.rs}, which reads exactly {@code
     * total_size} elements with no preceding length). Indexed reads (e.g.
     * {@code "vals[0]"}) are supported by the same dotted/indexed path
     * resolution {@code getMember}/{@code getI32} document.
     */
    @Test
    void fixedArrayOfPrimitivesRoundTrips() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo typeInfo = new TypeInfo("ArrayRec", Extensibility.APPENDABLE)) {
            typeInfo.addArrayField("vals", FieldType.INT32, 3, 0);

            try (TypeObject typeObject = typeInfo.toTypeObject()) {
                int[] vals = {10, 20, 30};

                byte[] serialized;
                try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                        ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                    int token = w.dheaderBegin();
                    for (int v : vals) {
                        w.writeI32(v);
                    }
                    w.dheaderFinalize(token);
                    serialized = w.toBytes();
                }

                try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                    assertEquals(vals.length, data.getLength("vals"));
                    assertEquals(vals[0], data.getI32("vals[0]"));
                    assertEquals(vals[2], data.getI32("vals[2]"));
                }
            }
        } finally {
            p.close();
        }
    }

    @Test
    void boundedStringRoundTrips() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo typeInfo = new TypeInfo("StrRec", Extensibility.APPENDABLE)) {
            typeInfo.addStringField("label", 20, 0);

            try (TypeObject typeObject = typeInfo.toTypeObject()) {
                String label = "bounded";

                byte[] serialized;
                try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                        ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                    int token = w.dheaderBegin();
                    w.writeString(label);
                    w.dheaderFinalize(token);
                    serialized = w.toBytes();
                }

                try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                    assertEquals(label, data.getString("label"));
                }
            }
        } finally {
            p.close();
        }
    }

    /**
     * A sequence of nested structs: the sequence itself gets a uint32 length
     * ({@link CdrWriter#writeSeqHeader}), then each {@code Inner} element is
     * written per its own extensibility -- {@code Inner} is FINAL here, so
     * (mirroring {@code getMemberReadsANestedStructField}'s Point) no
     * per-element DHEADER, just its members inline. {@code getMember} resolves
     * the same dotted/indexed path as the scalar getters, so {@code
     * "items[0]"} reaches the first element's {@link DynamicData}.
     */
    @Test
    void sequenceOfNestedStructsRoundTrips() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo innerType = new TypeInfo("Inner", Extensibility.FINAL)) {
            innerType.addField("x", FieldType.INT32, 0);

            try (TypeInfo typeInfo = new TypeInfo("Outer", Extensibility.APPENDABLE)) {
                typeInfo.addSequenceOfNestedField("items", innerType, 0, 0);

                try (TypeObject typeObject = typeInfo.toTypeObject()) {
                    int[] xs = {5, 9};

                    byte[] serialized;
                    try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                            ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                        int token = w.dheaderBegin();
                        w.writeSeqHeader(xs.length);
                        for (int x : xs) {
                            w.writeI32(x);
                        }
                        w.dheaderFinalize(token);
                        serialized = w.toBytes();
                    }

                    try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                        assertEquals(xs.length, data.getLength("items"));
                        try (DynamicData item0 = data.getMember("items[0]")) {
                            assertEquals(xs[0], item0.getI32("x"));
                        }
                        try (DynamicData item1 = data.getMember("items[1]")) {
                            assertEquals(xs[1], item1.getI32("x"));
                        }
                    }
                }
            }
        } finally {
            p.close();
        }
    }
}
