package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.nio.Buffer;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import org.junit.jupiter.api.Test;

/**
 * Publishes through the Java write path and receives with the raw C ABI.
 *
 * <p>The receive side is deliberately not a Java DataReader — that class does
 * not exist yet. Using the generated declarations directly keeps this test
 * about the write path.
 */
class WritePathEndToEndTest {

    private static final Charset UTF8 = Charset.forName("UTF-8");
    private static final int FIELD_INT32 = 5;
    private static final int FIELD_FLOAT64 = 12;
    private static final int FIELD_STRING = 13;

    private static byte[] utf8(String s) {
        return s.getBytes(UTF8);
    }

    /**
     * Takes one sample, polling until the deadline. Returns the bytes, or fails.
     *
     * <p>A poll with a deadline rather than a fixed sleep: discovery takes an
     * unpredictable moment, and a sleep long enough to be reliable is also long
     * enough to make the suite slow.
     */
    private static byte[] takeWithin(long reader, long millis) throws InterruptedException {
        ByteBuffer buffer = ByteBuffer.allocateDirect(4096).order(ByteOrder.nativeOrder());
        ByteBuffer size = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        ByteBuffer valid = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        long bufAddr = FfiAccess.directBufferAddress(buffer);
        long sizeAddr = FfiAccess.directBufferAddress(size);
        long validAddr = FfiAccess.directBufferAddress(valid);

        long deadline = System.nanoTime() + millis * 1_000_000L;
        while (System.nanoTime() < deadline) {
            int rc = FfiAccess.datareaderTakeSerialized(
                    reader, bufAddr, buffer.capacity(), sizeAddr, validAddr);
            if (rc == 0) {
                // valid_data_out is *mut bool — one byte, not four.
                assertNotEquals((byte) 0, valid.get(0), "sample must carry valid data");
                int n = (int) size.getLong(0);
                byte[] out = new byte[n];
                ((Buffer) buffer).position(0);
                buffer.get(out, 0, n);
                ((Buffer) buffer).position(0);
                return out;
            }
            Thread.sleep(20);
        }
        fail("no sample arrived within " + millis + " ms");
        return null;
    }

    /**
     * Checks the received bytes against what our encoder produced, tolerant
     * of exactly one thing: standard RTPS DATA-submessage 4-byte alignment
     * padding, and nothing else.
     *
     * <p>A naive {@code assertArrayEquals(expected, received)} fails for this
     * test's own literal sample — not because the write path is broken, but
     * because of a real, verified core behavior worth recording here rather
     * than silently working around: {@code Data::write_to}
     * (dds/src/rtps/messages/submessages/data.rs) pads the DATA submessage
     * body to a 4-byte boundary before it goes on the wire, and {@code
     * Data::deserialize} in the same file reconstructs {@code
     * serialized_data} as {@code buffer.slice(start_pos..)} — everything to
     * the end of the (already-padded) submessage — with no corresponding
     * trim. That padding then flows unmodified through the CacheChange into
     * {@code actual_size_out} and the copied buffer, so {@code
     * int2dds_datareader_take_serialized} hands back 1-3 extra trailing zero
     * bytes whenever the true payload length is not already a multiple of 4
     * — 38 bytes for this test's own record, so 2 padding bytes land in
     * {@code received} every time. {@code SampleInfo} carries no independent
     * length field that could be used instead, and no fix at the RTPS
     * framing layer is possible in general without becoming CDR-aware:
     * nothing on the wire records the true unpadded length separately from the
     * padded submessage boundary for a payload with no self-describing frame
     * of its own (this record is XCDR1 APPENDABLE, which — unlike XCDR2 —
     * has no DHEADER). See the task report for the full trace.
     *
     * <p>None of this is visible to a type-aware decoder: {@link
     * ConformanceRecord#deserializeCdr} and the core's own {@code
     * dynamic_sample_get_*} family both read exactly as many bytes as the
     * type's field layout calls for and simply never look at the trailing
     * padding, which is exactly what assertions 2 and 3 below confirm.
     * Only a raw byte-for-byte comparison — this one — can even see the
     * padding, so this is the one place it needs to be accounted for
     * explicitly rather than asserted away.
     *
     * <p>Still fails hard on anything this padding tolerance does not cover:
     * a missing or truncated sample (the length check below), corruption
     * anywhere in the logical payload (the prefix comparison), extra bytes
     * beyond the standard padding amount, or padding bytes that are not
     * actually zero.
     */
    private static void assertPayloadSurvivedTransport(byte[] expected, byte[] received) {
        int pad = (4 - (expected.length % 4)) % 4;
        assertEquals(expected.length + pad, received.length,
                "received length must be exactly the encoder's output length plus standard "
                        + "RTPS 4-byte DATA-submessage alignment padding (0-3 bytes)");
        byte[] receivedContent = new byte[expected.length];
        System.arraycopy(received, 0, receivedContent, 0, expected.length);
        assertArrayEquals(expected, receivedContent, "transport must not alter the payload");
        for (int i = expected.length; i < received.length; i++) {
            assertEquals((byte) 0, received[i],
                    "trailing alignment padding at index " + i + " must be zero");
        }
    }

    @Test
    void aSampleSurvivesTheWholeStack() throws InterruptedException {
        DomainParticipant p = new DomainParticipant(testDomain());
        long subscriber = 0L;
        long reader = 0L;
        long typeInfo = 0L;
        try {
            ConformanceRecord proto = new ConformanceRecord();
            Topic<ConformanceRecord> topic = p.createTopic("e2e_topic", proto);

            subscriber = FfiAccess.createSubscriber(p.handle(), 0L);
            assertNotEquals(0L, subscriber);
            reader = FfiAccess.createDataReader(subscriber, topic.handle(), 0L, 0L, 0);
            assertNotEquals(0L, reader);

            Publisher pub = p.createPublisher();
            DataWriterQos qos = new DataWriterQos();
            qos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataWriter<ConformanceRecord> writer = pub.createDataWriter(topic, qos);

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 4242;
            sent.value = -12.75d;
            sent.label = "센서/온도";

            // Publish repeatedly: the reader may not have matched on the first
            // write, and RELIABLE only helps once the match exists.
            byte[] received = null;
            long deadline = System.nanoTime() + 10_000L * 1_000_000L;
            while (System.nanoTime() < deadline && received == null) {
                writer.write(sent);
                try {
                    received = takeWithin(reader, 300);
                } catch (AssertionError retry) {
                    received = null;
                }
            }
            if (received == null) {
                fail("nothing was received within 10 seconds");
            }

            // 1. The bytes on the wire are exactly what our encoder produced.
            //
            // xcdr2=false, not the literal true the brief's own draft used:
            // `writer` above was built with a DataWriterQos that never calls
            // setDataRepresentation, so DataWriter.resolveXcdr2 falls back to
            // FfiAccess.defaultDataRepresentation() -- confirmed at runtime
            // for this task to be XCDR1 (0), the same core default
            // DataWriterTest.writeEncodesTheExactBytesHandedToNative already
            // established independently. Reconstructing `expected` with
            // xcdr2=true would build a different encoding (a leading
            // ENCAP_D_CDR2_LE byte, 0x09, instead of the ENCAP_CDR_LE writer
            // actually emits, 0x01) than what write() actually put on the
            // wire, which is exactly the mismatch this comparison exists to
            // catch -- not something this test should get wrong about its
            // own fixture. See the task report for the full trace.
            byte[] expected;
            try (CdrWriter w = CdrWriter.acquire(proto.extensibility(),
                    ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                sent.serializeCdr(w);
                expected = w.toBytes();
            }
            assertPayloadSurvivedTransport(expected, received);

            // 2. Our own reader decodes what came back off the wire.
            ConformanceRecord back = new ConformanceRecord();
            back.deserializeCdr(CdrReader.of(received));
            assertEquals(4242, back.id);
            assertEquals(-12.75d, back.value, 0.0d);
            assertEquals("센서/온도", back.label);

            // 3. The core's own deserializer decodes the received bytes too —
            //    not just the ones we handed it in process in the conformance test.
            typeInfo = FfiAccess.typeInfoCreate(utf8("ConformanceRecord"),
                    Extensibility.APPENDABLE.value());
            assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8("id"), FIELD_INT32, 0));
            assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8("value"), FIELD_FLOAT64, 0));
            assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8("label"), FIELD_STRING, 0));
            long typeObject = FfiAccess.typeInfoToTypeObject(typeInfo);
            assertNotEquals(0L, typeObject);

            ByteBuffer payload = ByteBuffer.allocateDirect(received.length)
                    .order(ByteOrder.nativeOrder());
            payload.put(received);
            ((Buffer) payload).position(0);
            ByteBuffer field = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            assertEquals(0, FfiAccess.dynamicSampleGetI32(
                    FfiAccess.directBufferAddress(payload), received.length, typeObject,
                    utf8("id"), FfiAccess.directBufferAddress(field)));
            assertEquals(4242, field.getInt(0));
        } finally {
            if (typeInfo != 0L) {
                FfiAccess.typeInfoDestroy(typeInfo);
            }
            if (reader != 0L) {
                FfiAccess.deleteDataReader(reader);
            }
            if (subscriber != 0L) {
                FfiAccess.deleteSubscriber(subscriber);
            }
            p.close();
        }
    }

    @Test
    void aReaderWithNoPublisherReportsNoData() {
        DomainParticipant p = new DomainParticipant(testDomain());
        long subscriber = 0L;
        long reader = 0L;
        try {
            Topic<ConformanceRecord> topic =
                    p.createTopic("e2e_empty", new ConformanceRecord());
            subscriber = FfiAccess.createSubscriber(p.handle(), 0L);
            reader = FfiAccess.createDataReader(subscriber, topic.handle(), 0L, 0L, 0);

            ByteBuffer buffer = ByteBuffer.allocateDirect(256).order(ByteOrder.nativeOrder());
            ByteBuffer size = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            ByteBuffer valid = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = FfiAccess.datareaderTakeSerialized(reader,
                    FfiAccess.directBufferAddress(buffer), buffer.capacity(),
                    FfiAccess.directBufferAddress(size),
                    FfiAccess.directBufferAddress(valid));
            assertTrue(rc != 0, "an empty reader must report a status, not a phantom sample");
        } finally {
            if (reader != 0L) {
                FfiAccess.deleteDataReader(reader);
            }
            if (subscriber != 0L) {
                FfiAccess.deleteSubscriber(subscriber);
            }
            p.close();
        }
    }
}
