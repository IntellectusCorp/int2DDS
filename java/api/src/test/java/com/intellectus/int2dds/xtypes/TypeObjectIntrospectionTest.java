package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.exceptions.DdsException;
import org.junit.jupiter.api.Test;

/**
 * Proves the runtime TypeObject introspection surface (member count, name,
 * lookup, per-member info, extensibility) reads back what {@link TypeInfo}
 * built -- a wrong native struct offset or {@link FieldType} kind mapping
 * would fail these assertions.
 */
class TypeObjectIntrospectionTest {

    @Test
    void introspectsAKnownStruct() {
        try (TypeInfo ti = new TypeInfo("Rec", Extensibility.APPENDABLE)) {
            ti.addField("id", FieldType.INT32, 0);
            ti.addField("value", FieldType.FLOAT64, 0);
            ti.addStringField("label", 0, 0);

            try (TypeObject to = ti.toTypeObject()) {
                assertEquals(3, to.memberCount());

                assertEquals("id", to.memberName(0));
                assertEquals("value", to.memberName(1));
                assertEquals("label", to.memberName(2));

                assertEquals(1, to.findMember("value"));

                assertEquals(Extensibility.APPENDABLE, to.extensibility());

                assertEquals(FieldType.INT32, to.memberInfo(0).kind());
                assertEquals(FieldType.FLOAT64, to.memberInfo(1).kind());
                assertEquals(FieldType.STRING, to.memberInfo(2).kind());
                assertEquals(0, to.memberInfo(0).memberId());
                assertEquals(1, to.memberInfo(1).memberId());
                assertEquals(2, to.memberInfo(2).memberId());
            }
        }
    }

    @Test
    void findMemberThrowsForAnUnknownName() {
        try (TypeInfo ti = new TypeInfo("Rec2", Extensibility.FINAL)) {
            ti.addField("id", FieldType.INT32, 0);

            try (TypeObject to = ti.toTypeObject()) {
                DdsException e = assertThrows(DdsException.class, () -> to.findMember("nope"));
                assertEquals(200, e.getCode()); // RET_DYNAMIC_FIELD_NOT_FOUND
            }
        }
    }
}
