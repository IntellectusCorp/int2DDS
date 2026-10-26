using System;
using System.Threading;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Qos;
using Int2Dds.Tests.Helpers;
using Xunit;

namespace Int2Dds.Tests
{
    public class InvalidDurationTests
    {
        /// <summary>
        /// Infinite spellings reach the native side as infinite and survive a GetQos/SetQos round
        /// trip, while a negative duration is rejected instead of silently becoming zero.
        /// </summary>
        [Fact]
        public void InfiniteDurationsRoundTripAndNegativeOnesAreRejected()
        {
            using (var participant = new DomainParticipant(77))
            using (var topic = participant.CreateTopic<HelloWorld>("InvalidDurationCs"))
            {
                var publisher = participant.CreatePublisher();
                var writer = publisher.CreateDataWriter(topic, new DataWriterQos
                {
                    Lifespan = new Lifespan(Timeout.InfiniteTimeSpan),
                    Reliability = new Reliability(ReliabilityKind.Reliable, TimeSpan.MaxValue),
                });

                writer.SetQos(writer.GetQos());
                writer.WaitForAcknowledgments(Timeout.InfiniteTimeSpan);

                Assert.Throws<DdsInvalidArgumentException>(() => publisher.CreateDataWriter(topic, new DataWriterQos
                {
                    Reliability = new Reliability(ReliabilityKind.Reliable, TimeSpan.FromSeconds(-1)),
                }));
            }
        }
    }
}
