using System;
using System.Threading;
using Int2Dds.Conditions;
using Int2Dds.Core;
using Int2Dds.Qos;

namespace HelloWorldPublisherWithTtl
{
    /// <summary>
    /// Mirrors <c>dds/examples/hello_world/hello_world_with_ttl.rs</c>:
    /// the DomainParticipant is created with a PropertyQosPolicy carrying
    /// the <c>int2dds.transport.UDPv4.multicast_ttl</c> entry, set via
    /// <see cref="Property.SetMulticastTtl"/>.
    /// </summary>
    class Program
    {
        private const byte MulticastTtl = 64;

        static void Main(string[] args)
        {
            Console.WriteLine($"=== HelloWorld Publisher with multicast TTL = {MulticastTtl} (C#) ===");

            var property = new Property();
            property.SetMulticastTtl(MulticastTtl);
            var qos = new ParticipantQos { Property = property };

            using var dp = new DomainParticipant(domainId: 0, qos: qos, name: "CSharpPublisherWithTtl");
            Console.WriteLine($"Created participant on domain {dp.DomainId} with TTL={MulticastTtl}");

            using var topic = dp.CreateTopic<HelloWorld>("hello_world_topic");
            Console.WriteLine($"Created topic: {topic.Name} ({topic.TypeName})");

            using var pub = dp.CreatePublisher();
            using var writer = pub.CreateDataWriter(topic);
            Console.WriteLine("Created publisher and data writer");

            Console.WriteLine("Waiting for subscriber...");
            using var statusCondition = writer.GetStatusCondition();
            statusCondition.EnabledStatuses = StatusMask.PublicationMatched;
            using var waitset = new WaitSet();
            waitset.Attach(statusCondition);

            while (writer.MatchedReaders == 0)
            {
                try { waitset.Wait(TimeSpan.FromSeconds(1)); }
                catch { /* timeout, retry */ }
            }

            Console.WriteLine($"Matched {writer.MatchedReaders} reader(s)");

            uint i = 0;
            while (true)
            {
                var sample = new HelloWorld(i, $"Hello from C# (ttl={MulticastTtl})! ({i})");
                writer.Write(sample);
                Console.WriteLine($"Published: index={sample.Index}, message='{sample.Message}'");
                Thread.Sleep(500);
                i++;
            }
        }
    }
}
