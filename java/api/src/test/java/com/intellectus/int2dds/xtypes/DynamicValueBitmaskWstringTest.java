package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/**
 * De-risk test for {@link DynamicValue#bitmask}, {@link DynamicValue#bitset}
 * and {@link DynamicValue#wstring}: build values of the last three
 * DynamicValue kinds and read them back, mirroring {@code
 * xml_dynamic_complex.rs}'s {@code Caps}/{@code Health}/{@code label} XML
 * (lines ~27-34, ~53), build (lines ~199-204, ~238) and read (lines ~365-370)
 * flow. wstring is read back with the existing {@link DynamicValue#asString}
 * -- a Unicode round-trip proves the wide-string path works.
 */
class DynamicValueBitmaskWstringTest {

    private static final String XML = "<types>\n"
            + " <module name=\"n\">\n"
            + "  <bitmask name=\"Caps\" bit_bound=\"8\">\n"
            + "   <bit_value name=\"A\" position=\"0\"/>\n"
            + "   <bit_value name=\"B\" position=\"1\"/>\n"
            + "  </bitmask>\n"
            + "  <bitset name=\"Health\">\n"
            + "   <bitfield name=\"battery\" bit_bound=\"7\" type=\"uint8\"/>\n"
            + "   <bitfield name=\"flags\" bit_bound=\"2\" type=\"uint8\"/>\n"
            + "  </bitset>\n"
            + "  <struct name=\"Rec\">\n"
            + "   <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "   <member name=\"caps\" type=\"nonBasic\" nonBasicTypeName=\"n::Caps\"/>\n"
            + "   <member name=\"health\" type=\"nonBasic\" nonBasicTypeName=\"n::Health\"/>\n"
            + "   <member name=\"label\" type=\"wstring\"/>\n"
            + "  </struct>\n"
            + " </module>\n"
            + "</types>\n";

    @Test
    void buildsAndReadsBackBitmaskBitsetAndWstring() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport recSupport = registry.getTypeSupport("n::Rec")) {
                try (DynamicData rec = DynamicData.create(recSupport)) {
                    rec.setU32("id", 1);

                    // Consumed by setValue -- do not close afterward.
                    rec.setValue("caps", DynamicValue.bitmask(0b11));
                    rec.setValue("health", DynamicValue.bitset((2L << 7) | 80));
                    rec.setValue("label", DynamicValue.wstring("로봇"));

                    try (DynamicValue capsV = rec.getValue("caps")) {
                        assertEquals(DynamicValueKind.BITMASK, capsV.kind());
                        assertEquals(0b11L, capsV.asBitmask());
                    }

                    try (DynamicValue healthV = rec.getValue("health")) {
                        assertEquals(DynamicValueKind.BITSET, healthV.kind());
                        assertEquals((2L << 7) | 80, healthV.asBitset());
                    }

                    try (DynamicValue labelV = rec.getValue("label")) {
                        assertEquals("로봇", labelV.asString());
                    }
                }
            }
        }
    }
}
