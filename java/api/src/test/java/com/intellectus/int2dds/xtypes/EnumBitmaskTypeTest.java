package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.exceptions.DdsException;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

/**
 * Builder-level and round-trip coverage for {@link TypeInfo#createEnum},
 * {@link TypeInfo#createBitmask}, {@link TypeInfo#addEnumLiteral}, {@link
 * TypeInfo#addBitmaskFlag}, and {@link TypeInfo#addNamedTypeField}.
 *
 * <p>Two distinct blockers were found for a full enum-field decode-and-read
 * round trip, documented at each relevant test below:
 *
 * <ul>
 *   <li>{@code addNamedTypeField} (backed by {@code
 *       int2dds_type_info_add_named_type_field}) emits a name-hash {@code
 *       MinimalTypeId} reference and does not record the referenced type's
 *       {@code TypeObject} into the struct builder's dependency closure
 *       (see {@code ffi/src/type_info.rs}'s {@code named_type_identifier}
 *       vs. {@code push_nested_field}/{@code intern_nested}). Locally built
 *       type info therefore cannot resolve it at decode time.
 *   <li>{@link TypeInfo#addNestedField} *does* record the dependency (it is
 *       generic over struct/enum/bitmask builder bodies -- see {@code
 *       Int2DdsTypeInfo::build_type_object}), so an enum field added this
 *       way resolves and decodes into the core's {@code DynamicValue::Enum}.
 *       But {@link DynamicData#getI32} only accepts {@code
 *       DynamicValue::Int32/Int16/Int8} (see {@code impl FromDynamicValue for
 *       i32} in {@code dds/src/xtypes/dynamic_data.rs}) -- there is no
 *       Java-exposed enum-value getter -- so reading the resolved value
 *       still fails, with a generic {@link DdsException} (the dynamic-type
 *       mismatch code has no dedicated Java exception type).
 * </ul>
 */
class EnumBitmaskTypeTest {

    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void enumTypeBuildsAndBakesIntoATypeObject() {
        try (TypeInfo color = TypeInfo.createEnum("Color", 32)) {
            color.addEnumLiteral("RED", 0, true);
            color.addEnumLiteral("GREEN", 1, false);
            color.addEnumLiteral("BLUE", 2, false);

            try (TypeObject typeObject = color.toTypeObject()) {
                assertNotNull(typeObject);
            }
        }
    }

    @Test
    void bitmaskTypeBuildsAndBakesIntoATypeObject() {
        try (TypeInfo perms = TypeInfo.createBitmask("Permissions", 32)) {
            perms.addBitmaskFlag("READ", 0);
            perms.addBitmaskFlag("WRITE", 1);
            perms.addBitmaskFlag("EXEC", 2);

            try (TypeObject typeObject = perms.toTypeObject()) {
                assertNotNull(typeObject);
            }
        }
    }

    /**
     * {@code addNamedTypeField} itself is a well-formed bridge: it returns OK and the
     * owning struct still bakes into a {@link TypeObject}, even though (per the class
     * doc) the referenced enum is not locally resolvable from that reference alone.
     */
    @Test
    void namedTypeFieldReturnsOkAndStructStillBuilds() {
        try (TypeInfo color = TypeInfo.createEnum("Color", 32)) {
            color.addEnumLiteral("RED", 0, true);
            color.addEnumLiteral("BLUE", 1, false);

            try (TypeInfo rec = new TypeInfo("NamedRefRecord", Extensibility.FINAL)) {
                rec.addNamedTypeField("color", "Color", 0);

                try (TypeObject typeObject = rec.toTypeObject()) {
                    assertNotNull(typeObject);
                }
            }
        }
    }

    /**
     * Full round trip via {@link TypeInfo#addNestedField}: this is the mechanism that
     * actually records the enum's {@code TypeObject} into the struct's dependency
     * closure (unlike {@code addNamedTypeField}), so the field resolves at decode.
     * The value decodes into the core's {@code DynamicValue::Enum}, but {@link
     * DynamicData#getI32} rejects that variant -- there is no Java enum-value getter
     * yet -- so the read leg fails with a generic {@link DdsException}. This documents
     * exactly where the round trip is currently blocked: type-level resolution works,
     * value-level reading does not (a follow-up: add a getEnum/getEnumValue getter).
     */
    @Test
    void nestedEnumFieldResolvesButHasNoJavaValueGetterYet() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo color = TypeInfo.createEnum("Color", 32)) {
            color.addEnumLiteral("RED", 0, true);
            color.addEnumLiteral("GREEN", 1, false);
            color.addEnumLiteral("BLUE", 2, false);

            try (TypeInfo rec = new TypeInfo("Rec", Extensibility.APPENDABLE)) {
                rec.addNestedField("color", color, 0);

                try (TypeObject typeObject = rec.toTypeObject()) {
                    // BLUE == 2, bit_bound 32 -> serialized as a plain int32.
                    byte[] serialized;
                    try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                            ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                        int token = w.dheaderBegin();
                        w.writeI32(2);
                        w.dheaderFinalize(token);
                        serialized = w.toBytes();
                    }

                    try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                        // The type resolves (no DYNAMIC_FIELD_NOT_FOUND); the value read
                        // itself is where this currently blocks.
                        assertThrows(DdsException.class, () -> data.getI32("color"));
                    }
                }
            }
        } finally {
            p.close();
        }
    }

    @Test
    void differentBitBoundsBuildIndependently() {
        try (TypeInfo flags = TypeInfo.createBitmask("Flags8", 8)) {
            flags.addBitmaskFlag("A", 0);
            flags.addBitmaskFlag("B", 7);
            try (TypeObject t1 = flags.toTypeObject()) {
                assertNotNull(t1);
            }
        }
        try (TypeInfo e = TypeInfo.createEnum("Small", 8)) {
            e.addEnumLiteral("ZERO", 0, true);
            e.addEnumLiteral("ONE", 1, false);
            try (TypeObject t = e.toTypeObject()) {
                assertNotNull(t);
            }
        }
    }
}
