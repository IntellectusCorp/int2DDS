package kr.co.intellectus.int2dds.core;

import static kr.co.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.charset.StandardCharsets;
import kr.co.intellectus.int2dds.qos.DataReaderQos;
import kr.co.intellectus.int2dds.qos.DataWriterQos;
import kr.co.intellectus.int2dds.qos.ParticipantQos;
import kr.co.intellectus.int2dds.qos.UserData;
import kr.co.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/** USER_DATA set on an entity reads back through {@code getQos()}. */
class UserDataQosTest {

    @Test
    void participantUserDataReadsBackAfterCreateAndSetQos() {
        ParticipantQos created = new ParticipantQos();
        created.setUserData(userData("created"));

        try (DomainParticipant p = new DomainParticipant(testDomain(), created)) {
            assertEquals(userData("created"), p.getQos().getUserData());

            ParticipantQos changed = new ParticipantQos();
            changed.setUserData(userData("changed"));
            p.setQos(changed);

            assertEquals(userData("changed"), p.getQos().getUserData());
        }
    }

    @Test
    void writerAndReaderUserDataReadBack() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> t = p.createTopic("user_data_qos", new ConformanceRecord());

            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setUserData(userData("writer"));
            DataWriter<ConformanceRecord> w = p.createPublisher().createDataWriter(t, writerQos);

            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setUserData(userData("reader"));
            DataReader<ConformanceRecord> r =
                    p.createSubscriber().createDataReader(t, ConformanceRecord::new, readerQos);

            assertEquals(userData("writer"), w.getQos().getUserData());
            assertEquals(userData("reader"), r.getQos().getUserData());
        }
    }

    @Test
    void unsetUserDataReadsBackEmpty() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            assertEquals(new UserData(new byte[0]), p.getQos().getUserData());
        }
    }

    private static UserData userData(String text) {
        return new UserData(text.getBytes(StandardCharsets.UTF_8));
    }
}
