using System;
using Int2Dds.Conditions;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Qos;

namespace HelloWorldSubscriberWithTtl
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
            Console.WriteLine($"=== HelloWorld Subscriber with multicast TTL = {MulticastTtl} (C#) ===");

            var property = new Property();
            property.SetMulticastTtl(MulticastTtl);
            var qos = new ParticipantQos { Property = property };

            using var dp = new DomainParticipant(domainId: 0, qos: qos, name: "CSharpSubscriberWithTtl");
            Console.WriteLine($"Created participant on domain {dp.DomainId} with TTL={MulticastTtl}");

            using var topic = dp.CreateTopic<HelloWorld>("hello_world_topic");
            Console.WriteLine($"Created topic: {topic.Name} ({topic.TypeName})");

            using var sub = dp.CreateSubscriber();
            using var reader = sub.CreateDataReader(topic);
            Console.WriteLine("Created subscriber and data reader");

            using var statusCond = reader.GetStatusCondition();
            statusCond.EnabledStatuses = StatusMask.SubscriptionMatched;

            Console.WriteLine("Waiting for publisher...");
            using var waitset = new WaitSet();
            waitset.Attach(statusCond);

            while (reader.MatchedWriters == 0)
            {
                try { waitset.Wait(TimeSpan.FromSeconds(1)); }
                catch (DdsTimeoutException) { /* timeout, retry */ }
            }

            Console.WriteLine($"Matched {reader.MatchedWriters} writer(s)");

            statusCond.EnabledStatuses = StatusMask.DataAvailable;

            Console.WriteLine("Waiting for data...");
            int samplesReceived = 0;
            int timeoutCount = 0;

            while (timeoutCount < 3)
            {
                foreach (var sample in reader.Take())
                {
                    if (sample.ValidData && sample.Data != null)
                    {
                        Console.WriteLine($"Received: index={sample.Data.Index}, message='{sample.Data.Message}'");
                        samplesReceived++;
                    }
                    else
                    {
                        Console.WriteLine("Received dispose/unregister notification");
                    }
                    timeoutCount = 0;
                }

                try
                {
                    waitset.Wait(TimeSpan.FromSeconds(2));

                    foreach (var sample in reader.Take())
                    {
                        if (sample.ValidData && sample.Data != null)
                        {
                            Console.WriteLine($"Received: index={sample.Data.Index}, message='{sample.Data.Message}'");
                            samplesReceived++;
                        }
                        else
                        {
                            Console.WriteLine("Received dispose/unregister notification");
                        }
                    }

                    timeoutCount = 0;
                }
                catch (DdsTimeoutException)
                {
                    timeoutCount++;
                    Console.WriteLine($"No data received (timeout {timeoutCount}/3)");
                }
            }

            Console.WriteLine($"Done. Received {samplesReceived} samples.");
        }
    }
}
