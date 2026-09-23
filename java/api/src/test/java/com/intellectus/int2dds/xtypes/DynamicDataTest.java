package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

/**
 * End-to-end proof of the DynamicData read path: build a runtime type with
 * {@link TypeInfo}, encode a sample with the same {@link CdrWriter} the write
 * path uses, decode it back with {@link DomainParticipant#dynamicDataFromSample},
 * and read the fields back through the {@link DynamicData} object API.
 */
class DynamicDataTest {

    /** The domain this JVM's tests use, isolated from other runs. */
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void readsFieldsWrittenThroughTheSameCdrEncoding() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo typeInfo = new TypeInfo("ConformanceRecord", Extensibility.APPENDABLE)) {
            typeInfo.addField("id", FieldType.INT32, 0);
            typeInfo.addField("value", FieldType.FLOAT64, 0);
            typeInfo.addField("label", FieldType.STRING, 0);

            try (TypeObject typeObject = typeInfo.toTypeObject()) {
                ConformanceRecord sent = new ConformanceRecord();
                sent.id = 7;
                sent.value = 3.5d;
                sent.label = "hello";

                byte[] serialized;
                try (CdrWriter w = CdrWriter.acquire(sent.extensibility(),
                        ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                    sent.serializeCdr(w);
                    serialized = w.toBytes();
                }

                try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                    assertEquals(7, data.getI32("id"));
                    assertEquals(3.5d, data.getF64("value"), 0.0d);
                    assertEquals("hello", data.getString("label"));
                }
            }
        } finally {
            p.close();
        }
    }

    @Test
    void readsEveryRemainingPrimitiveGetter() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo typeInfo = new TypeInfo("PrimitiveConformanceRecord", Extensibility.APPENDABLE)) {
            typeInfo.addField("boolVal", FieldType.BOOL, 0);
            typeInfo.addField("i8Val", FieldType.INT8, 0);
            typeInfo.addField("u8Val", FieldType.UINT8, 0);
            typeInfo.addField("i16Val", FieldType.INT16, 0);
            typeInfo.addField("u16Val", FieldType.UINT16, 0);
            typeInfo.addField("i64Val", FieldType.INT64, 0);
            typeInfo.addField("u64Val", FieldType.UINT64, 0);
            typeInfo.addField("u32Val", FieldType.UINT32, 0);
            typeInfo.addField("f32Val", FieldType.FLOAT32, 0);
            typeInfo.addField("char8Val", FieldType.CHAR8, 0);

            try (TypeObject typeObject = typeInfo.toTypeObject()) {
                byte i8Val = -42;
                int u8Val = 200;
                short i16Val = -1234;
                int u16Val = 50000;
                long i64Val = -123456789012345L;
                long u64Bits = 0xFFFFFFFFFFFFFF10L; // raw bits, no unsigned magnitude claimed
                int u32Bits = 0xFFFFFFF0; // raw bits, no unsigned magnitude claimed
                float f32Val = 3.25f;
                byte char8Val = (byte) 'Z';

                byte[] serialized;
                try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                        ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                    int token = w.dheaderBegin();
                    w.writeBool(true);
                    w.writeI8(i8Val);
                    w.writeU8(u8Val);
                    w.writeI16(i16Val);
                    w.writeU16(u16Val);
                    w.writeI64(i64Val);
                    w.writeU64(u64Bits);
                    w.writeU32(u32Bits);
                    w.writeF32(f32Val);
                    w.writeI8(char8Val);
                    w.dheaderFinalize(token);
                    serialized = w.toBytes();
                }

                try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                    assertEquals(true, data.getBool("boolVal"));
                    assertEquals(i8Val, data.getI8("i8Val"));
                    assertEquals(u8Val, data.getU8("u8Val"));
                    assertEquals(i16Val, data.getI16("i16Val"));
                    assertEquals(u16Val, data.getU16("u16Val"));
                    assertEquals(i64Val, data.getI64("i64Val"));
                    assertEquals(u64Bits, data.getU64("u64Val"));
                    assertEquals(u32Bits, data.getU32("u32Val"));
                    assertEquals(f32Val, data.getF32("f32Val"), 0.0f);
                    assertEquals(char8Val, data.getChar8("char8Val"));
                }
            }
        } finally {
            p.close();
        }
    }

    @Test
    void readsSequenceLength() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo typeInfo = new TypeInfo("SequenceRecord", Extensibility.APPENDABLE)) {
            typeInfo.addSequenceField("values", FieldType.INT32, 0, 0);

            try (TypeObject typeObject = typeInfo.toTypeObject()) {
                int[] values = {10, 20, 30};

                byte[] serialized;
                try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                        ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                    int token = w.dheaderBegin();
                    w.writeSeqHeader(values.length);
                    for (int v : values) {
                        w.writeI32(v);
                    }
                    w.dheaderFinalize(token);
                    serialized = w.toBytes();
                }

                try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                    assertEquals(values.length, data.getLength("values"));
                }
            }
        } finally {
            p.close();
        }
    }

    /**
     * {@code getMember} on a struct-typed nested field, decoded through {@link
     * TypeInfo#addNestedField}'s dependency closure. Exercises the fix in {@code
     * int2dds_dynamic_data_from_sample} (ffi/src/dynamic.rs): it now registers {@code
     * TypeObject.deps} into a throwaway {@code TypeRegistry} before resolving, the same
     * way {@code decode_flat} already did -- previously this path ignored {@code deps}
     * entirely and any struct-typed nested field failed to decode.
     */
    @Test
    void getMemberReadsANestedStructField() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo pointType = new TypeInfo("Point", Extensibility.FINAL)) {
            pointType.addField("x", FieldType.INT32, 0);
            pointType.addField("y", FieldType.INT32, 0);

            try (TypeInfo typeInfo = new TypeInfo("NestedRecord", Extensibility.APPENDABLE)) {
                typeInfo.addNestedField("point", pointType, 0);

                try (TypeObject typeObject = typeInfo.toTypeObject()) {
                    int x = 11;
                    int y = 22;

                    byte[] serialized;
                    try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                            ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                        int token = w.dheaderBegin();
                        w.writeI32(x);
                        w.writeI32(y);
                        w.dheaderFinalize(token);
                        serialized = w.toBytes();
                    }

                    try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                        try (DynamicData point = data.getMember("point")) {
                            assertEquals(x, point.getI32("x"));
                            assertEquals(y, point.getI32("y"));
                        }
                    }
                }
            }
        } finally {
            p.close();
        }
    }
}
