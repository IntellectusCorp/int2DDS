package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Exercises the zero-copy transactional serialized-write API: {@link
 * DataWriter#prepareSerializedWrite} / {@link SerializedWriteBuffer}. Reuses
 * {@link SerializedIoTest}'s writer/reader round-trip fixture shape to get
 * real CDR bytes, then publishes them through a DDS-owned buffer instead of a
 * caller-built {@code byte[]}.
 */
class SerializedWriteTxnTest {

    private static final long DATA_TIMEOUT_NANOS = 5_000_000_000L;
    private static final long POLL_SLEEP_MILLIS = 20;

    @Test
    void zeroCopyPrepareCommitRoundTripsThroughTypedDecode() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedWriteTxnRoundTrip", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();

            DataWriter<ConformanceRecord> w1 = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r1 = sub.createDataReader(topic, ConformanceRecord::new);

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 21;
            sent.value = 4.5;
            sent.label = "zero-copy";

            byte[] cdr = null;
            long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                w1.write(sent);
                cdr = r1.takeSerialized();
                if (cdr != null && cdr.length > 0) {
                    break;
                }
                Thread.sleep(POLL_SLEEP_MILLIS);
            }
            assertNotNull(cdr, "no serialized sample within 5s -- discovery or receive path");
            assertTrue(cdr.length > 0, "takeSerialized should return non-empty CDR bytes");

            // Second writer/reader pair: zero-copy publish those exact bytes
            // through prepareSerializedWrite, then decode with the ordinary
            // typed path.
            DataWriter<ConformanceRecord> w2 = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r2 = sub.createDataReader(topic, ConformanceRecord::new);

            Sample<ConformanceRecord> got = null;
            deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                try (SerializedWriteBuffer buf = w2.prepareSerializedWrite(cdr.length + 64)) {
                    buf.buffer().put(cdr);
                    buf.commit();
                }
                got = r2.take();
                if (got != null) {
                    break;
                }
                Thread.sleep(POLL_SLEEP_MILLIS);
            }
            assertNotNull(got, "no sample within 5s after prepareSerializedWrite/commit");
            assertTrue(got.info().validData(), "sample should carry valid data");
            assertEquals(sent.id, got.data().id);
            assertEquals(sent.value, got.data().value);
            assertEquals(sent.label, got.data().label);

            r1.setListener(null, null);
            r2.setListener(null, null);
        }
    }

    @Test
    void closeWithoutCommitAbortsAndPublishesNothing() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedWriteTxnAbort", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();

            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            // Establish the match with an ordinary write/take first, so the
            // absence of a sample below is not just "never matched".
            ConformanceRecord warm = new ConformanceRecord();
            warm.id = 1;
            warm.label = "warm";
            long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            Sample<ConformanceRecord> matched = null;
            while (System.nanoTime() < deadline) {
                w.write(warm);
                matched = r.take();
                if (matched != null) {
                    break;
                }
                Thread.sleep(POLL_SLEEP_MILLIS);
            }
            assertNotNull(matched, "writer/reader failed to match within 5s");

            // Prepare, write into the buffer, but never commit -- close()
            // (via try-with-resources) must abort instead of publishing.
            try (SerializedWriteBuffer buf = w.prepareSerializedWrite(128)) {
                buf.buffer().put(new byte[] {0, 1, 0, 0, 1, 2, 3, 4});
                // intentionally no commit()
            }

            long pollDeadline = System.nanoTime() + 500_000_000L;
            boolean sawData = false;
            while (System.nanoTime() < pollDeadline) {
                if (r.hasData()) {
                    sawData = true;
                    break;
                }
                Thread.sleep(POLL_SLEEP_MILLIS);
            }
            assertFalse(sawData, "an aborted (never committed) buffer must not publish anything");
            assertNull(r.take(), "no new sample should be available after an aborted write");

            r.setListener(null, null);
        }
    }

    @Test
    void bufferAndCommitAreGuardedAfterCommit() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedWriteTxnGuard", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);

            SerializedWriteBuffer buf = w.prepareSerializedWrite(64);
            buf.buffer().put(new byte[] {0, 1, 0, 0});
            buf.commit();

            assertThrows(IllegalStateException.class, buf::buffer,
                    "buffer() must be guarded once committed");
            assertThrows(IllegalStateException.class, buf::commit,
                    "a second commit() must be guarded");

            // Idempotent no-op close after commit must not throw.
            buf.close();
        }
    }
}
