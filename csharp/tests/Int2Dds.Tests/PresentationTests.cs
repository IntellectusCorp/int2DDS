using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Qos;
using Int2Dds.Tests.Helpers;
using Xunit;

namespace Int2Dds.Tests
{
    public class PresentationTests
    {
        private static readonly TimeSpan Deadline = TimeSpan.FromSeconds(10);

        [Fact]
        public void PublisherAndSubscriberPresentation_ReadsBack()
        {
            using (var dp = new DomainParticipant(74))
            {
                var publisher = dp.CreatePublisher(new PublisherQos
                {
                    Presentation = new Presentation(PresentationAccessScopeKind.Topic, true, false),
                });
                var subscriber = dp.CreateSubscriber(new SubscriberQos
                {
                    Presentation = new Presentation(PresentationAccessScopeKind.Group, false, true),
                });

                var pubQos = publisher.GetQos();
                Assert.Equal(PresentationAccessScopeKind.Topic, pubQos.Presentation!.AccessScope);
                Assert.True(pubQos.Presentation.CoherentAccess);
                Assert.False(pubQos.Presentation.OrderedAccess);

                var subQos = subscriber.GetQos();
                Assert.Equal(PresentationAccessScopeKind.Group, subQos.Presentation!.AccessScope);
                Assert.False(subQos.Presentation.CoherentAccess);
                Assert.True(subQos.Presentation.OrderedAccess);
            }
        }

        [Fact]
        public void GroupCoherentOrderedAccess_ReturnsReadersInSampleOrder()
        {
            var group = new Presentation(PresentationAccessScopeKind.Group, true, true);
            var writerQos = new DataWriterQos
            {
                Reliability = new Reliability(ReliabilityKind.Reliable),
                History = new History(HistoryKind.KeepAll),
            };
            var readerQos = new DataReaderQos
            {
                Reliability = new Reliability(ReliabilityKind.Reliable),
                History = new History(HistoryKind.KeepAll),
            };

            using (var pubDp = new DomainParticipant(74))
            using (var subDp = new DomainParticipant(74))
            {
                var pubTopicA = pubDp.CreateTopic<HelloWorld>("PresentationGroupA_Cs");
                var pubTopicB = pubDp.CreateTopic<HelloWorld>("PresentationGroupB_Cs");
                var subTopicA = subDp.CreateTopic<HelloWorld>("PresentationGroupA_Cs");
                var subTopicB = subDp.CreateTopic<HelloWorld>("PresentationGroupB_Cs");

                var publisher = pubDp.CreatePublisher(new PublisherQos { Presentation = group });
                var subscriber = subDp.CreateSubscriber(new SubscriberQos { Presentation = group });
                var writerA = publisher.CreateDataWriter(pubTopicA, writerQos);
                var writerB = publisher.CreateDataWriter(pubTopicB, writerQos);
                var readerA = subscriber.CreateDataReader(subTopicA, readerQos);
                var readerB = subscriber.CreateDataReader(subTopicB, readerQos);

                var sw = Stopwatch.StartNew();
                while (writerA.GetPublicationMatchedStatus().currentCount == 0
                    || writerB.GetPublicationMatchedStatus().currentCount == 0)
                {
                    Assert.True(sw.Elapsed < Deadline, "writers not matched within 10s");
                    Thread.Sleep(10);
                }

                Assert.Throws<DdsPreconditionNotMetException>(() => publisher.EndCoherentChanges());
                Assert.Throws<DdsPreconditionNotMetException>(() => subscriber.EndAccess());
                Assert.Throws<DdsPreconditionNotMetException>(() => subscriber.GetDataReaders());

                publisher.BeginCoherentChanges();
                writerA.Write(new HelloWorld(1, "a1"));
                writerB.Write(new HelloWorld(2, "b1"));
                writerA.Write(new HelloWorld(3, "a2"));
                publisher.EndCoherentChanges();

                sw.Restart();
                IReadOnlyList<object> readers;
                while (true)
                {
                    subscriber.BeginAccess();
                    try
                    {
                        readers = subscriber.GetDataReaders();
                    }
                    finally
                    {
                        subscriber.EndAccess();
                    }
                    if (readers.Count == 3) break;
                    Assert.True(sw.Elapsed < Deadline, $"expected 3 readers within 10s, got {readers.Count}");
                    Thread.Sleep(10);
                }

                Assert.Same(readerA, readers[0]);
                Assert.Same(readerB, readers[1]);
                Assert.Same(readerA, readers[2]);

                // GROUP access scope rejects a take outside an access block.
                Assert.Throws<DdsPreconditionNotMetException>(() => readerA.TakeOne());

                // Taking one sample per listed reader, in list order, yields the write order.
                var messages = new List<string>();
                subscriber.BeginAccess();
                try
                {
                    foreach (var reader in subscriber.GetDataReaders())
                    {
                        var sample = ((DataReader<HelloWorld>)reader).TakeOne();
                        Assert.NotNull(sample);
                        messages.Add(sample!.Value.Data!.Message);
                    }
                }
                finally
                {
                    subscriber.EndAccess();
                }

                Assert.Equal(new[] { "a1", "b1", "a2" }, messages);

                var publication = subDp.TakeDiscoveredPublications()
                    .Single(p => p.TopicName == "PresentationGroupA_Cs");
                using (publication)
                {
                    Assert.Equal(2, publication.PresentationAccessScope);
                    Assert.True(publication.PresentationCoherentAccess);
                    Assert.True(publication.PresentationOrderedAccess);
                }
            }
        }
    }
}
