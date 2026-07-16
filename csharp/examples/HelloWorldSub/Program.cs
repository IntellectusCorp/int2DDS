using System;
using System.Threading;
using Int2Dds.Conditions;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Qos;

namespace HelloWorldSub
{
    class Program
    {
        static void Main(string[] args)
        {
            // Reliability is selectable on the CLI (default BEST_EFFORT, --reliable for RELIABLE)
            bool reliable = Array.IndexOf(args, "--reliable") >= 0;
            // Domain id is selectable on the CLI (-d/--domain, default 0), matching the Rust example.
            int domainId = ParseDomain(args);
            Console.WriteLine("=== HelloWorld Subscriber (C#) ===");
            Console.WriteLine($"QoS: {(reliable ? "RELIABLE" : "BEST_EFFORT")}");

            // Run until Ctrl-C, then clean up gracefully (matches the Rust example).
            using var stop = new ManualResetEventSlim(false);
            Console.CancelKeyPress += (_, e) => { e.Cancel = true; stop.Set(); };

            using var dp = new DomainParticipant(domainId: domainId, name: "CSharpSubscriber");
            Console.WriteLine($"Created participant on domain {dp.DomainId}");

            using var topic = dp.CreateTopic<HelloWorld>("hello_world_topic");
            Console.WriteLine($"Created topic: {topic.Name} ({topic.TypeName})");

            using var sub = dp.CreateSubscriber();
            var readerQos = new DataReaderQos
            {
                Reliability = new Reliability(
                    reliable ? ReliabilityKind.Reliable : ReliabilityKind.BestEffort,
                    reliable ? TimeSpan.FromMilliseconds(100) : (TimeSpan?)null),
            };
            using var reader = sub.CreateDataReader(topic, readerQos);
            Console.WriteLine("Created subscriber and data reader");

            // Get StatusCondition and configure for discovery phase
            using var statusCond = reader.GetStatusCondition();
            statusCond.EnabledStatuses = StatusMask.SubscriptionMatched;

            // Wait for publisher to connect
            Console.WriteLine("Waiting for publisher...");
            using var waitset = new WaitSet();
            waitset.Attach(statusCond);

            while (reader.MatchedWriters == 0 && !stop.IsSet)
            {
                try { waitset.Wait(TimeSpan.FromSeconds(1)); }
                catch (DdsTimeoutException) { /* timeout, retry */ }
            }

            if (stop.IsSet)
            {
                return;
            }

            Console.WriteLine($"Matched {reader.MatchedWriters} writer(s)");

            // Switch to DATA_AVAILABLE for data reception
            statusCond.EnabledStatuses = StatusMask.DataAvailable;

            // Receive samples until Ctrl-C
            Console.WriteLine("Waiting for data...");
            int samplesReceived = 0;

            while (!stop.IsSet)
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
                }

                try { waitset.Wait(TimeSpan.FromSeconds(2)); }
                catch (DdsTimeoutException) { /* no data yet, keep waiting */ }
            }

            Console.WriteLine($"Done. Received {samplesReceived} samples.");
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
