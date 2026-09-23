package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

import com.intellectus.int2dds.cdr.Extensibility;
import org.junit.jupiter.api.Test;

/**
 * Coverage for {@link TypeInfo#addWstringField}, {@link
 * TypeInfo#addSequenceOfNamedField}, and {@link TypeInfo#addArrayOfNamedField}.
 */
class TypeInfoNamedWstringTest {

    /**
     * Fully local: the wstring field's kind is readable straight off the baked
     * {@link TypeObject}, no discovery/decode needed, so this checks the actual
     * {@link FieldType#WSTRING} member kind, not just field presence.
     */
    @Test
    void wstringFieldRoundTripsIntoTheTypeObject() {
        try (TypeInfo ti = new TypeInfo("WstrRec", Extensibility.APPENDABLE)) {
            ti.addField("id", FieldType.INT32, 0);
            ti.addWstringField("label", 0, 0);

            try (TypeObject to = ti.toTypeObject()) {
                assertEquals(2, to.memberCount());

                int idx = to.findMember("label");
                assertEquals(FieldType.WSTRING, to.memberInfo(idx).kind());
            }
        }
    }

    /**
     * The element type is resolved by name at discovery time, so this cannot fully
     * round-trip locally (per {@link TypeInfo#addSequenceOfNamedField}'s and {@link
     * TypeInfo#addArrayOfNamedField}'s discovery-resolution caveat). This still is a
     * genuine can-fail assertion: a bad bridge/param order would throw here or leave
     * the field unresolvable by name.
     */
    @Test
    void namedSequenceAndArrayFieldsReturnOkAndResolveByName() {
        try (TypeInfo ti = new TypeInfo("NamedRefRec", Extensibility.APPENDABLE)) {
            ti.addSequenceOfNamedField("items", "SomeType", 0, 0);
            ti.addArrayOfNamedField("arr", "SomeType", 4, 0);

            try (TypeObject to = ti.toTypeObject()) {
                assertNotNull(to);
                assertEquals(2, to.memberCount());

                int itemsIdx = to.findMember("items");
                int arrIdx = to.findMember("arr");
                assertEquals(FieldType.SEQUENCE, to.memberInfo(itemsIdx).kind());
                assertEquals(FieldType.ARRAY, to.memberInfo(arrIdx).kind());
            }
        }
    }
}
