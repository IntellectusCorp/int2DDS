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
    public class GroupDataTests
    {
        [Fact]
        public void PublisherAndSubscriberGroupData_ReadBackAfterCreateAndSetQos()
        {
            var pubCreated = new byte[] { 1, 2 };
            var subCreated = new byte[] { 3, 4, 5 };
            using (var dp = new DomainParticipant(75))
            {
                var publisher = dp.CreatePublisher(new PublisherQos { GroupData = new GroupData(pubCreated) });
                var subscriber = dp.CreateSubscriber(new SubscriberQos { GroupData = new GroupData(subCreated) });

                Assert.Equal(pubCreated, publisher.GetQos().GroupData!.Data);
                Assert.Equal(subCreated, subscriber.GetQos().GroupData!.Data);

                var pubUpdated = new byte[] { 6, 7, 8, 9 };
                var subUpdated = new byte[] { 10 };
                publisher.SetQos(new PublisherQos { GroupData = new GroupData(pubUpdated) });
                subscriber.SetQos(new SubscriberQos { GroupData = new GroupData(subUpdated) });

                Assert.Equal(pubUpdated, publisher.GetQos().GroupData!.Data);
                Assert.Equal(subUpdated, subscriber.GetQos().GroupData!.Data);

                publisher.SetQos(new PublisherQos { GroupData = new GroupData(new byte[0]) });
                subscriber.SetQos(new SubscriberQos { GroupData = new GroupData(new byte[0]) });

                Assert.Empty(publisher.GetQos().GroupData!.Data);
                Assert.Empty(subscriber.GetQos().GroupData!.Data);
            }
        }

        [Fact]
        public void MatchedEndpoints_ReportPeerGroupData()
        {
            const string topicName = "GroupDataMatchedCs";
            var pubGroupData = new byte[] { 0xC1, 0xC2 };
            var subGroupData = new byte[] { 0xD1, 0xD2, 0xD3 };

            using (var pubDp = new DomainParticipant(75))
            using (var subDp = new DomainParticipant(75))
            using (var pubTopic = pubDp.CreateTopic<HelloWorld>(topicName))
            using (var subTopic = subDp.CreateTopic<HelloWorld>(topicName))
            {
                var writer = pubDp.CreatePublisher(new PublisherQos { GroupData = new GroupData(pubGroupData) })
                    .CreateDataWriter(pubTopic);
                var reader = subDp.CreateSubscriber(new SubscriberQos { GroupData = new GroupData(subGroupData) })
                    .CreateDataReader(subTopic);

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
                    Assert.Equal(subGroupData, subscription.GroupData);
                }

                var publication = subDp.TakeDiscoveredPublications().Single(p => p.TopicName == topicName);
                using (publication)
                {
                    Assert.Equal(pubGroupData, publication.GroupData);
                }
            }
        }
    }
}
