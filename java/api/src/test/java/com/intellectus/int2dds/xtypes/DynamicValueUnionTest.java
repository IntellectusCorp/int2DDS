package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/**
 * De-risk test for {@link DynamicValue#union} / {@link
 * DynamicValue#unionDiscriminator} / {@link DynamicValue#unionValue}: build a
 * union value entirely off {@code DynamicValue}, set it into a {@code
 * DynamicData} field, and read the discriminator/branch back out --
 * mirroring {@code xml_dynamic_complex.rs}'s {@code cmd} build/read. {@link
 * DynamicValue#union} consumes TWO handles (discriminator and value) on
 * success, unconditionally; a clean run (no JVM crash) is itself part of what
 * this proves -- a wrongly consumed or double-closed handle here would
 * SIGSEGV.
 */
class DynamicValueUnionTest {

    private static final String XML = "<types>\n"
            + " <module name=\"n\">\n"
            + "  <union name=\"Cmd\">\n"
            + "   <discriminator type=\"int32\"/>\n"
            + "   <case><caseDiscriminator value=\"0\"/><member name=\"speed\" type=\"float32\"/></case>\n"
            + "   <case><caseDiscriminator value=\"1\"/><member name=\"stop\" type=\"boolean\"/></case>\n"
            + "  </union>\n"
            + "  <struct name=\"Rec\">\n"
            + "   <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "   <member name=\"cmd\" type=\"nonBasic\" nonBasicTypeName=\"n::Cmd\"/>\n"
            + "  </struct>\n"
            + " </module>\n"
            + "</types>\n";

    @Test
    void buildsAUnionValueAndReadsDiscriminatorAndBranchBackOut() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport support = registry.getTypeSupport("n::Rec")) {
                try (DynamicData data = DynamicData.create(support)) {
                    data.setU32("id", 1);

                    // discriminator 0 selects the float32 "speed" branch.
                    DynamicValue cmd = DynamicValue.union(DynamicValue.i32(0), DynamicValue.f32(2.5f));
                    // cmd (and the discriminator/value it was built from) is
                    // consumed by setValue below -- do not close it afterward.
                    data.setValue("cmd", cmd);

                    try (DynamicValue cmdV = data.getValue("cmd")) {
                        assertEquals(DynamicValueKind.UNION, cmdV.kind());

                        try (DynamicValue disc = cmdV.unionDiscriminator()) {
                            assertEquals(0, disc.asI32());
                        }
                        try (DynamicValue branch = cmdV.unionValue()) {
                            assertEquals(2.5f, branch.asF32());
                        }
                    }
                }
            }
        }
    }
}
