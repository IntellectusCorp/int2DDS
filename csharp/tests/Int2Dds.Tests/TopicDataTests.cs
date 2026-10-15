using System;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using Int2Dds.Core;
using Int2Dds.Qos;
using Int2Dds.Tests.Helpers;
using Xunit;

namespace Int2Dds.Tests
{
    public class TopicDataTests
    {
        [Fact]
        public void TopicData_ReadsBackAfterCreateAndSetQos()
        {
            var created = new byte[] { 1, 2, 3 };
            using (var dp = new DomainParticipant(76))
            using (var topic = dp.CreateTopic<HelloWorld>(
                "TopicDataReadBackCs", new TopicQos { TopicData = new TopicData(created) }))
            {
                Assert.Equal(created, topic.GetQos().TopicData!.Data);

                var updated = new byte[] { 4, 5, 6, 7 };
                topic.SetQos(new TopicQos { TopicData = new TopicData(updated) });
                Assert.Equal(updated, topic.GetQos().TopicData!.Data);

                topic.SetQos(new TopicQos { TopicData = new TopicData(new byte[0]) });
                Assert.Empty(topic.GetQos().TopicData!.Data);
            }
        }

        [Fact]
        public void MatchedEndpoints_ReportPeerTopicData()
        {
            const string topicName = "TopicDataMatchedCs";
            var pubTopicData = new byte[] { 0xA1, 0xA2 };
            var subTopicData = new byte[] { 0xB1, 0xB2, 0xB3 };

            using (var pubDp = new DomainParticipant(76))
            using (var subDp = new DomainParticipant(76))
            using (var pubTopic = pubDp.CreateTopic<HelloWorld>(
                topicName, new TopicQos { TopicData = new TopicData(pubTopicData) }))
            using (var subTopic = subDp.CreateTopic<HelloWorld>(
                topicName, new TopicQos { TopicData = new TopicData(subTopicData) }))
            {
                var writer = pubDp.CreatePublisher().CreateDataWriter(pubTopic);
                var reader = subDp.CreateSubscriber().CreateDataReader(subTopic);

                var sw = Stopwatch.StartNew();
                while (writer.GetPublicationMatchedStatus().currentCount == 0
                    || reader.GetSubscriptionMatchedStatus().currentCount == 0)
                {
                    Assert.True(sw.Elapsed < TimeSpan.FromSeconds(10), "no match within 10s");
                    Thread.Sleep(10);
                }

                var subscription = pubDp.TakeDiscoveredSubscriptions().Single(s => s.TopicName == topicName);
                using (subscription)
                {
                    Assert.Equal(subTopicData, subscription.TopicData);
                }

                var publication = subDp.TakeDiscoveredPublications().Single(p => p.TopicName == topicName);
                using (publication)
                {
                    Assert.Equal(pubTopicData, publication.TopicData);
                }
            }
        }
    }
}
