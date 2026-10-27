package kr.co.intellectus.int2dds.core;

import static kr.co.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.Collections;
import kr.co.intellectus.int2dds.cdr.CdrReader;
import kr.co.intellectus.int2dds.conditions.InstanceState;
import kr.co.intellectus.int2dds.conditions.ReadCondition;
import kr.co.intellectus.int2dds.conditions.SampleState;
import kr.co.intellectus.int2dds.conditions.ViewState;
import kr.co.intellectus.int2dds.exceptions.DdsPreconditionNotMetException;
import kr.co.intellectus.int2dds.qos.DataReaderQos;
import kr.co.intellectus.int2dds.qos.DataWriterQos;
import kr.co.intellectus.int2dds.qos.History;
import kr.co.intellectus.int2dds.qos.HistoryKind;
import kr.co.intellectus.int2dds.types.ConformanceRecord;
import kr.co.intellectus.int2dds.xtypes.FieldType;
import org.junit.jupiter.api.Test;

class LoanedSamplesTest {

    private static final long TIMEOUT_NANOS = 5_000_000_000L;
    private static final int ANY = SampleState.ANY;

    private static DataWriterQos writerQos() {
        DataWriterQos qos = new DataWriterQos();
        qos.setHistory(new History(HistoryKind.KEEP_ALL, 0));
        return qos;
    }

    private static DataReaderQos readerQos() {
        DataReaderQos qos = new DataReaderQos();
        qos.setHistory(new History(HistoryKind.KEEP_ALL, 0));
        return qos;
    }

    private static ConformanceRecord record(int id) {
        ConformanceRecord r = new ConformanceRecord();
        r.id = id;
        r.label = "r" + id;
        return r;
    }

    private static void awaitMatch(DataReader<ConformanceRecord> r) throws InterruptedException {
        long deadline = System.nanoTime() + TIMEOUT_NANOS;
        while (r.getSubscriptionMatchedStatus().currentCount() == 0) {
            assertTrue(System.nanoTime() < deadline, "no match within 5s");
            Thread.sleep(10);
        }
    }

    private static void awaitSamples(DataReader<ConformanceRecord> r, int count)
            throws InterruptedException {
        long deadline = System.nanoTime() + TIMEOUT_NANOS;
        while (true) {
            try (LoanedSamples<ConformanceRecord> loan =
                    r.readLoaned(100, ANY, ViewState.ANY, InstanceState.ANY)) {
                if (loan.size() == count) {
                    return;
                }
            }
            assertTrue(System.nanoTime() < deadline, "samples not cached within 5s");
            Thread.sleep(10);
        }
    }

    private static int decodeId(LoanedSample s) {
        ConformanceRecord decoded = new ConformanceRecord();
        decoded.deserializeCdr(CdrReader.of(s.serializedData()));
        return decoded.id;
    }

    /** Lend, refuse a foreign return, block closing, return, then close. */
    @Test
    void loanScenario() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic(
                            "LoanedScenario",
                            new ConformanceRecord(),
                            Collections.singletonList(new TopicFieldDescriptor("id", FieldType.INT32, true)));
            DataWriter<ConformanceRecord> w = p.createPublisher().createDataWriter(topic, writerQos());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new, readerQos());
            DataReader<ConformanceRecord> other =
                    sub.createDataReader(topic, ConformanceRecord::new, readerQos());
            awaitMatch(r);
            awaitMatch(other);
            w.write(record(1));
            w.write(record(2));
            awaitSamples(r, 2);

            ReadCondition rc = r.createReadCondition(ANY, ViewState.ANY, InstanceState.ANY);

            // Instance variants lend one instance: by handle, or iterating from null.
            InstanceHandle firstHandle;
            InstanceHandle secondHandle;
            try (LoanedSamples<ConformanceRecord> all =
                    r.readLoaned(100, ANY, ViewState.ANY, InstanceState.ANY)) {
                firstHandle = new InstanceHandle(all.get(0).info().instanceHandle());
                secondHandle = new InstanceHandle(all.get(1).info().instanceHandle());
            }
            try (LoanedSamples<ConformanceRecord> byHandle =
                    r.readInstanceLoaned(secondHandle, 100, ANY, ViewState.ANY, InstanceState.ANY)) {
                assertEquals(1, byHandle.size());
                assertEquals(2, decodeId(byHandle.get(0)));
            }
            try (LoanedSamples<ConformanceRecord> next =
                    r.readNextInstanceLoaned(null, 100, ANY, ViewState.ANY, InstanceState.ANY)) {
                assertEquals(1, next.size());
                assertEquals(1, decodeId(next.get(0)));
            }
            try (LoanedSamples<ConformanceRecord> next =
                    r.readNextInstanceWithConditionLoaned(firstHandle, rc, 100)) {
                assertEquals(1, next.size());
                assertEquals(2, decodeId(next.get(0)));
            }

            LoanedSamples<ConformanceRecord> loan = r.takeWithConditionLoaned(rc, 100);
            assertEquals(2, loan.size());
            LoanedSample first = loan.get(0);
            assertTrue(first.info().validData());
            assertEquals(1, decodeId(first));
            assertEquals(2, decodeId(loan.get(1)));

            assertThrows(DdsPreconditionNotMetException.class, () -> other.returnLoan(loan));
            assertEquals(1, decodeId(first));
            rc.close();
            assertThrows(DdsPreconditionNotMetException.class, r::close);

            loan.close();
            loan.close();
            assertThrows(IllegalStateException.class, first::serializedData);
            assertThrows(IllegalStateException.class, loan::size);
            assertThrows(
                    IllegalArgumentException.class,
                    () -> r.readLoaned(0, ANY, ViewState.ANY, InstanceState.ANY));
            // An empty loan holds nothing native: the reader closes, then the loan still closes.
            LoanedSamples<ConformanceRecord> empty = r.readLoaned(-1, ANY, ViewState.ANY, InstanceState.ANY);
            assertEquals(0, empty.size());
            r.close();
            empty.close();
        }
    }
}
