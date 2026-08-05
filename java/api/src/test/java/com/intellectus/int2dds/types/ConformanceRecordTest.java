package com.intellectus.int2dds.types;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import org.junit.jupiter.api.Test;

class ConformanceRecordTest {

    @Test
    void extensibilityCarriesTheWireValueNotTheOrdinal() {
        // The native side takes these as ints. Today ordinal() happens to agree,
        // which is exactly why it needs to be explicit — a reordering of the
        // enum constants would silently change the wire value.
        assertEquals(0, Extensibility.FINAL.value());
        assertEquals(1, Extensibility.APPENDABLE.value());
        assertEquals(2, Extensibility.MUTABLE.value());
    }

    @Test
    void extensibilityRoundTripsThroughItsWireValue() {
        for (Extensibility e : Extensibility.values()) {
            assertEquals(e, Extensibility.fromValue(e.value()));
        }
    }

    @Test
    void anUnknownExtensibilityValueIsRejected() {
        assertThrows(IllegalArgumentException.class, () -> Extensibility.fromValue(7));
    }

    @Test
    void theRecordRoundTripsThroughItsOwnCdr() {
        ConformanceRecord sent = new ConformanceRecord();
        sent.id = 42;
        sent.value = 2.5d;
        sent.label = "센서/온도";

        byte[] bytes;
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            sent.serializeCdr(w);
            bytes = w.toBytes();
        }

        ConformanceRecord back = new ConformanceRecord();
        back.deserializeCdr(CdrReader.of(bytes));

        assertEquals(42, back.id);
        assertEquals(2.5d, back.value, 0.0d);
        assertEquals("센서/온도", back.label);
    }

    @Test
    void theTypeNameIsStable() {
        // The topic is created with this name, and the core matches publishers
        // to subscribers on it. It must not depend on the JVM's class naming.
        assertEquals("ConformanceRecord", new ConformanceRecord().typeName());
    }

    @Test
    void theExtensibilityMatchesWhatSerializeCdrEmits() {
        // serializeCdr wraps the struct in a DHEADER, which is what APPENDABLE
        // means. Topic creation passes this value to the core, so a mismatch
        // here produces a topic the core decodes with the wrong reader.
        assertEquals(Extensibility.APPENDABLE, new ConformanceRecord().extensibility());
    }
}
