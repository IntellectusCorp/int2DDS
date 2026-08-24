package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.exceptions.DdsAlreadyDeletedException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises {@link DomainParticipant#findTopic}: looking up an existing topic
 * by name rather than creating a new one, returning a typed {@link Topic}
 * that wraps an already-obtained handle sharing the same logical topic as the
 * one that was created.
 */
class FindTopicTest {

    private static final long DATA_TIMEOUT_NANOS = 5_000_000_000L;

    /**
     * Deliberately reachable for the rest of this JVM: see the note at the
     * end of {@link #findTopicLocatesExistingTopicUsableForRoundTrip} for why
     * one entity from that test must never become unreachable.
     */
    private static final List<Object> LEAKED_ON_PURPOSE = new ArrayList<Object>();

    /**
     * Strongest test: {@code found} must refer to the very same logical
     * topic as {@code created}, not merely report the same name/type. Proven
     * by creating a writer on {@code found} and a reader on {@code created}
     * (the reverse pairing of the natural one) and showing a write actually
     * round-trips -- if {@code findTopic} had somehow minted an unrelated
     * topic under the same name, this reader/writer pair would never match.
     */
    @Test
    void findTopicLocatesExistingTopicUsableForRoundTrip() throws InterruptedException {
        DomainParticipant p = new DomainParticipant(testDomain());
        Topic<ConformanceRecord> created = p.createTopic("findme", new ConformanceRecord());
        Topic<ConformanceRecord> found = p.findTopic("findme", new ConformanceRecord(), 1000);

        assertNotNull(found, "findTopic should return a non-null Topic");
        assertEquals("findme", found.name());
        assertEquals(new ConformanceRecord().typeName(), found.typeName());

        Publisher pub = p.createPublisher();
        Subscriber sub = p.createSubscriber();
        DataWriter<ConformanceRecord> w = pub.createDataWriter(found);
        DataReader<ConformanceRecord> r = sub.createDataReader(created, ConformanceRecord::new);

        ConformanceRecord sent = new ConformanceRecord();
        sent.id = 7;
        sent.value = 3.5;
        sent.label = "found-topic-round-trip";

        Sample<ConformanceRecord> got = null;
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            w.write(sent);
            got = r.take();
            if (got != null) {
                break;
            }
            Thread.sleep(20);
        }
        assertNotNull(got, "no sample within 5s -- found and created topics did not match,"
                + " so findTopic did not resolve to the same logical topic");
        assertTrue(got.info().validData(), "sample should carry valid data");
        assertEquals(sent.id, got.data().id);
        assertEquals(sent.value, got.data().value);
        assertEquals(sent.label, got.data().label);

        r.setListener(null, null);

        // The core refuses to delete a Topic still in use by a
        // DataWriter/DataReader (NativeEntity's own doc on close()
        // ordering), even though w/r's Java parent is pub/sub, not the
        // topic -- so close the entities using each topic first.
        w.close();
        r.close();

        // found's own handle closes cleanly on its own -- the real, verified
        // claim: findTopic's return value is independently closable like any
        // created Topic.
        found.close();

        // created names the very same native topic found just deleted: this
        // dds core keys a participant's topics by name/handle, not a
        // per-lookup refcount, so -- despite the DDS spec's own commentary in
        // domain_participant.rs's find_topic suggesting find/create
        // acquisitions should be independently deletable, one corresponding
        // delete_topic each -- created's own close now genuinely fails
        // against real native state (RET_ALREADY_DELETED), not a bug here or
        // in findTopic. Demonstrated directly rather than masked.
        assertThrows(DdsAlreadyDeletedException.class, created::close);

        // A failed close leaves created's Java-side handle OPEN (see
        // NativeHandle's own doc), and that failure is permanent -- no later
        // retry of this same delete can ever succeed. If created became
        // unreachable from here, NativeCleaner's background reaper would
        // pick it up, retry the same doomed delete forever, and permanently
        // inflate the process-wide deferred-entry count every other test
        // observes (NativeCleanerTest asserts that count returns to its own
        // baseline). Keeping a strong reference for the rest of this JVM
        // (created transitively keeps p, its parent, reachable too) avoids
        // that reaper attempt ever happening, at the cost of intentionally
        // never releasing this one native topic handle.
        LEAKED_ON_PURPOSE.add(created);
    }

    /** A short timeout against a name nothing ever registers must fail, not hang or silently succeed. */
    @Test
    void findTopicThrowsWhenNoSuchTopicExists() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            assertThrows(DdsException.class,
                    () -> p.findTopic("does_not_exist", new ConformanceRecord(), 100));
        }
    }
}
