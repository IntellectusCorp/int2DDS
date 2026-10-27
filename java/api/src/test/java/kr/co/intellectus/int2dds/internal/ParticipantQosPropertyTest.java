package kr.co.intellectus.int2dds.internal;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.Charset;
import java.util.function.LongConsumer;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;
import kr.co.intellectus.int2dds.qos.ParticipantQos;
import kr.co.intellectus.int2dds.qos.Property;
import kr.co.intellectus.int2dds.qos.PropertyEntry;
import org.junit.jupiter.api.Test;

/**
 * Reads participant QoS properties back through the C ABI's callback-driven iterator: a JNI
 * trampoline on JDK 8-21, an FFM upcall on 22+. Both swallow failures into an empty result, so only
 * asserting on the actual entries tells a working path from a silently broken one.
 */
class ParticipantQosPropertyTest {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private static void withParticipantQos(LongConsumer body) {
        long handle = FfiAccess.createParticipantQos();
        assertNotEquals(0L, handle, "the FFI must hand back a QoS handle");
        try {
            body.accept(handle);
        } finally {
            FfiAccess.destroyParticipantQos(handle);
        }
    }

    private static void add(long h, String name, String value) {
        ReturnCodes.check(
                FfiAccess.participantQosAddProperty(h, utf8(name), utf8(value), false));
    }

    private static byte[] utf8(String s) {
        return s.getBytes(UTF8);
    }

    @Test
    void prefixedPropertiesComeBackAsAlternatingNameValuePairs() {
        withParticipantQos(
                h -> {
                    add(h, "int2dds.test.alpha", "1");
                    add(h, "int2dds.test.empty", "");
                    add(h, "int2dds.test.utf8", "참여자-λ");
                    add(h, "other.beta", "2");

                    byte[][] flat =
                            FfiAccess.participantQosPropertiesWithPrefix(
                                    h, utf8("int2dds.test."));
                    assertEquals(6, flat.length, "three name/value pairs, nothing else");

                    java.util.Map<String, String> got = new java.util.TreeMap<>();
                    for (int i = 0; i < flat.length; i += 2) {
                        got.put(new String(flat[i], UTF8), new String(flat[i + 1], UTF8));
                    }
                    assertEquals("1", got.get("int2dds.test.alpha"));
                    // An empty value still occupies its slot rather than shifting the pairs.
                    assertEquals("", got.get("int2dds.test.empty"));
                    // UTF-8 bytes cross unchanged, never through modified UTF-8.
                    assertArrayEquals(
                            utf8("참여자-λ"), utf8(got.get("int2dds.test.utf8")));
                });
    }

    @Test
    void aNullPrefixIsRejectedAsAnEmptyResult() {
        withParticipantQos(
                h -> {
                    add(h, "int2dds.test.alpha", "1");
                    assertEquals(0, FfiAccess.participantQosPropertiesWithPrefix(h, null).length);
                });
    }

    @Test
    void readParticipantQosReturnsWhatWasApplied() {
        withParticipantQos(
                h -> {
                    ParticipantQos in = new ParticipantQos();
                    Property property = new Property();
                    property.add("int2dds.test.roundtrip", "value", false);
                    in.setProperty(property);
                    QosMarshal.applyParticipantQos(h, in);

                    ParticipantQos out = QosMarshal.readParticipantQos(h);
                    boolean found = false;
                    for (PropertyEntry e : out.getProperty().getEntries()) {
                        if (e.getName().equals("int2dds.test.roundtrip")) {
                            assertEquals("value", e.getValue());
                            found = true;
                        }
                    }
                    assertTrue(found, "applied property must read back: " + out.getProperty());
                });
    }
}
