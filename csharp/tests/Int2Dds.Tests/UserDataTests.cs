using Int2Dds.Core;
using Int2Dds.Qos;
using Int2Dds.Tests.Helpers;
using Xunit;

namespace Int2Dds.Tests
{
    public class UserDataTests
    {
        [Fact]
        public void ParticipantUserData_ReadsBackAfterCreateAndSetQos()
        {
            var created = new byte[] { 1, 2, 3 };
            using (var dp = new DomainParticipant(75, new ParticipantQos { UserData = new UserData(created) }))
            {
                Assert.Equal(created, dp.GetQos().UserData!.Data);

                var updated = new byte[] { 9, 8, 7, 6 };
                dp.SetQos(new ParticipantQos { UserData = new UserData(updated) });
                Assert.Equal(updated, dp.GetQos().UserData!.Data);

                dp.SetQos(new ParticipantQos { UserData = new UserData(new byte[0]) });
                Assert.Empty(dp.GetQos().UserData!.Data);
            }
        }

        [Fact]
        public void EndpointUserData_ClearedBySetQosWithEmptyData()
        {
            using (var dp = new DomainParticipant(75))
            using (var topic = dp.CreateTopic<HelloWorld>("UserDataClearCs"))
            {
                var writer = dp.CreatePublisher().CreateDataWriter(
                    topic, new DataWriterQos { UserData = new UserData(new byte[] { 0x10 }) });
                var reader = dp.CreateSubscriber().CreateDataReader(
                    topic, new DataReaderQos { UserData = new UserData(new byte[] { 0x20 }) });

                writer.SetQos(new DataWriterQos { UserData = new UserData(new byte[0]) });
                reader.SetQos(new DataReaderQos { UserData = new UserData(new byte[0]) });

                Assert.Empty(writer.GetQos().UserData!.Data);
                Assert.Empty(reader.GetQos().UserData!.Data);
            }
        }

        [Fact]
        public void EndpointUserData_ReadsBack()
        {
            var writerData = new byte[] { 0x10, 0x20 };
            var readerData = new byte[] { 0x30, 0x40, 0x50 };

            using (var dp = new DomainParticipant(75))
            using (var topic = dp.CreateTopic<HelloWorld>("UserDataEndpointsCs"))
            {
                var writer = dp.CreatePublisher().CreateDataWriter(
                    topic, new DataWriterQos { UserData = new UserData(writerData) });
                var reader = dp.CreateSubscriber().CreateDataReader(
                    topic, new DataReaderQos { UserData = new UserData(readerData) });

                Assert.Equal(writerData, writer.GetQos().UserData!.Data);
                Assert.Equal(readerData, reader.GetQos().UserData!.Data);
            }
        }

        [Fact]
        public void UnsetUserData_ReadsBackEmpty()
        {
            using (var dp = new DomainParticipant(75))
            using (var topic = dp.CreateTopic<HelloWorld>("UserDataUnsetCs"))
            {
                var writer = dp.CreatePublisher().CreateDataWriter(topic);
                var reader = dp.CreateSubscriber().CreateDataReader(topic);

                Assert.Empty(dp.GetQos().UserData!.Data);
                Assert.Empty(writer.GetQos().UserData!.Data);
                Assert.Empty(reader.GetQos().UserData!.Data);
            }
        }
    }
}
