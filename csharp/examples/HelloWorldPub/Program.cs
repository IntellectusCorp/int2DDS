using System;
using System.Threading;
using Int2Dds.Conditions;
using Int2Dds.Core;
using Int2Dds.Qos;

namespace HelloWorldPub
{
    class Program
    {
        static void Main(string[] args)
        {
            // Reliability is selectable on the CLI (default BEST_EFFORT, --reliable for RELIABLE)
            bool reliable = Array.IndexOf(args, "--reliable") >= 0;
            // Domain id is selectable on the CLI (-d/--domain, default 0), matching the Rust example.
            int domainId = ParseDomain(args);
            Console.WriteLine("=== HelloWorld Publisher (C#) ===");
            Console.WriteLine($"QoS: {(reliable ? "RELIABLE" : "BEST_EFFORT")}");

            using var dp = new DomainParticipant(domainId: domainId, name: "CSharpPublisher");
            Console.WriteLine($"Created participant on domain {dp.DomainId}");

            using var topic = dp.CreateTopic<HelloWorld>("hello_world_topic");
            Console.WriteLine($"Created topic: {topic.Name} ({topic.TypeName})");

            using var pub = dp.CreatePublisher();
            var writerQos = new DataWriterQos
            {
                Reliability = new Reliability(
                    reliable ? ReliabilityKind.Reliable : ReliabilityKind.BestEffort,
                    reliable ? TimeSpan.FromMilliseconds(100) : (TimeSpan?)null),
            };
            using var writer = pub.CreateDataWriter(topic, writerQos);
            Console.WriteLine("Created publisher and data writer");

            // Wait for subscriber to connect
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

            // Publish samples
            uint i = 0;
            while (true)
            {
                var sample = new HelloWorld { Index = i, Message = $"Hello from C#! ({i})" };
                writer.Write(sample);
                Console.WriteLine($"Published: index={sample.Index}, message='{sample.Message}'");
                Thread.Sleep(1000);
                i++;
            }
        }

        // Parse "-d N" / "--domain N" from the CLI, defaulting to 0 (mirrors the Rust example's -d/--domain).
        static int ParseDomain(string[] args)
        {
            for (int i = 0; i < args.Length - 1; i++)
            {
                if ((args[i] == "--domain" || args[i] == "-d") && int.TryParse(args[i + 1], out int d))
                {
                    return d;
                }
            }
            return 0;
        }
    }
}
