using System;
using System.Diagnostics;
using System.Threading;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Qos;
using Int2Dds.Tests.Helpers;
using Xunit;

namespace Int2Dds.Tests
{
    public class IgnoreTests
    {
        private static readonly TimeSpan MatchTimeout = TimeSpan.FromSeconds(5);
        private static readonly TimeSpan NoDataWindow = TimeSpan.FromSeconds(1);

        /// <summary>
        /// An ignored participant stays unmatched after it re-announces itself, and its data is not received.
        /// </summary>
        [Fact]
        public void IgnoreParticipantUnmatchesAndDropsItsData()
        {
            using (var localParticipant = new DomainParticipant(74))
            using (var remoteParticipant = new DomainParticipant(74))
            using (var localTopic = localParticipant.CreateTopic<HelloWorld>("IgnoreParticipantCs"))
            using (var remoteTopic = remoteParticipant.CreateTopic<HelloWorld>("IgnoreParticipantCs"))
            {
                var reader = localParticipant.CreateSubscriber().CreateDataReader(localTopic);
                var remoteWriter = remoteParticipant.CreatePublisher().CreateDataWriter(remoteTopic);

                Assert.Throws<DdsInvalidArgumentException>(
                    () => localParticipant.IgnoreParticipant(localParticipant.GetInstanceHandle()));

                WaitUntil(() => reader.GetSubscriptionMatchedStatus().currentCount == 1, "reader did not match");

                localParticipant.IgnoreParticipant(remoteParticipant.GetInstanceHandle());
                WaitUntil(() => reader.GetSubscriptionMatchedStatus().currentCount == 0, "reader still matched after ignore");

                var participantQos = remoteParticipant.GetQos();
                participantQos.UserData = new UserData(new byte[] { 1 });
                remoteParticipant.SetQos(participantQos);
                remoteWriter.Write(new HelloWorld(1, "ignored"));

                Thread.Sleep(NoDataWindow);
                Assert.Empty(reader.Take());
                Assert.Equal(0, reader.GetSubscriptionMatchedStatus().currentCount);
            }
        }

        /// <summary>
        /// An ignored publication stays unmatched after it re-announces itself, and its data is not received.
        /// </summary>
        [Fact]
        public void IgnorePublicationUnmatchesAndDropsItsData()
        {
            using (var localParticipant = new DomainParticipant(75))
            using (var remoteParticipant = new DomainParticipant(75))
            using (var localTopic = localParticipant.CreateTopic<HelloWorld>("IgnorePublicationCs"))
            using (var localOnlyTopic = localParticipant.CreateTopic<HelloWorld>("IgnorePublicationLocalCs"))
            using (var remoteTopic = remoteParticipant.CreateTopic<HelloWorld>("IgnorePublicationCs"))
            {
                var reader = localParticipant.CreateSubscriber().CreateDataReader(localTopic);
                var localWriter = localParticipant.CreatePublisher().CreateDataWriter(localOnlyTopic);
                var remoteWriter = remoteParticipant.CreatePublisher().CreateDataWriter(remoteTopic);

                Assert.Throws<DdsInvalidArgumentException>(
                    () => localParticipant.IgnorePublication(localWriter.GetInstanceHandle()));

                WaitUntil(() => reader.GetSubscriptionMatchedStatus().currentCount == 1, "reader did not match");

                localParticipant.IgnorePublication(remoteWriter.GetInstanceHandle());
                WaitUntil(() => reader.GetSubscriptionMatchedStatus().currentCount == 0, "reader still matched after ignore");

                var writerQos = remoteWriter.GetQos();
                writerQos.UserData = new UserData(new byte[] { 1 });
                remoteWriter.SetQos(writerQos);
                remoteWriter.Write(new HelloWorld(1, "ignored"));

                Thread.Sleep(NoDataWindow);
                Assert.Empty(reader.Take());
                Assert.Equal(0, reader.GetSubscriptionMatchedStatus().currentCount);
            }
        }

        /// <summary>
        /// An ignored subscription stays unmatched after it re-announces itself, and receives no data.
        /// </summary>
        [Fact]
        public void IgnoreSubscriptionUnmatchesAndSendsItNoData()
        {
            using (var localParticipant = new DomainParticipant(76))
            using (var remoteParticipant = new DomainParticipant(76))
            using (var localTopic = localParticipant.CreateTopic<HelloWorld>("IgnoreSubscriptionCs"))
            using (var localOnlyTopic = localParticipant.CreateTopic<HelloWorld>("IgnoreSubscriptionLocalCs"))
            using (var remoteTopic = remoteParticipant.CreateTopic<HelloWorld>("IgnoreSubscriptionCs"))
            {
                var writer = localParticipant.CreatePublisher().CreateDataWriter(localTopic);
                var localReader = localParticipant.CreateSubscriber().CreateDataReader(localOnlyTopic);
                var remoteReader = remoteParticipant.CreateSubscriber().CreateDataReader(remoteTopic);

                Assert.Throws<DdsInvalidArgumentException>(
                    () => localParticipant.IgnoreSubscription(localReader.GetInstanceHandle()));

                WaitUntil(() => writer.GetPublicationMatchedStatus().currentCount == 1, "writer did not match");

                localParticipant.IgnoreSubscription(remoteReader.GetInstanceHandle());
                WaitUntil(() => writer.GetPublicationMatchedStatus().currentCount == 0, "writer still matched after ignore");

                var readerQos = remoteReader.GetQos();
                readerQos.UserData = new UserData(new byte[] { 1 });
                remoteReader.SetQos(readerQos);
                writer.Write(new HelloWorld(1, "ignored"));

                Thread.Sleep(NoDataWindow);
                Assert.Empty(remoteReader.Take());
                Assert.Equal(0, writer.GetPublicationMatchedStatus().currentCount);
            }
        }

        private static void WaitUntil(Func<bool> condition, string failureMessage)
        {
            var stopwatch = Stopwatch.StartNew();
            while (!condition())
            {
                Assert.True(stopwatch.Elapsed < MatchTimeout, failureMessage);
                Thread.Sleep(10);
            }
        }
    }
}
