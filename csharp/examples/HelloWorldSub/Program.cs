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
        const string TopicName = "hello_world_topic";

        static void Main(string[] args)
        {
            // Reliability is selectable on the CLI (default BEST_EFFORT, --reliable for RELIABLE)
            bool reliable = Array.IndexOf(args, "--reliable") >= 0;
            // Domain id is selectable on the CLI (-d/--domain, default 0), matching the Rust example.
            int domainId = ParseDomain(args);
            // Run until Ctrl-C, then clean up gracefully (matches the Rust example).
            using var stop = new ManualResetEventSlim(false);
            Console.CancelKeyPress += (_, e) => { e.Cancel = true; stop.Set(); };

            using var dp = new DomainParticipant(domainId: domainId, name: "CSharpSubscriber");

            using var topic = dp.CreateTopic<HelloWorld>(TopicName);

            using var sub = dp.CreateSubscriber();
            var readerQos = new DataReaderQos
            {
                Reliability = new Reliability(
                    reliable ? ReliabilityKind.Reliable : ReliabilityKind.BestEffort,
                    reliable ? TimeSpan.FromMilliseconds(100) : (TimeSpan?)null),
            };
            using var reader = sub.CreateDataReader(topic, readerQos);

            var rqos = reader.GetQos();
            Console.WriteLine($"[subscriber INFO] domain_id: {domainId}, topic: {TopicName}");
            Console.WriteLine($"[subscriber qos] reliability: {rqos.Reliability?.Kind}, durability: {rqos.Durability?.Kind}, history: {HistoryText(rqos.History)}");

            // Get StatusCondition and configure for discovery phase
            using var statusCond = reader.GetStatusCondition();
            statusCond.EnabledStatuses = StatusMask.SubscriptionMatched;

            // Wait for publisher to connect
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

            Console.WriteLine("Publisher matched!");

            // Switch to DATA_AVAILABLE for data reception
            statusCond.EnabledStatuses = StatusMask.DataAvailable;

            // Receive samples until Ctrl-C
            int samplesReceived = 0;

            while (!stop.IsSet)
            {
                foreach (var sample in reader.Take())
                {
                    if (sample.ValidData && sample.Data != null)
                    {
                        Console.WriteLine($"Read sample: HelloWorld {{ index: {sample.Data.Index}, message: \"{sample.Data.Message}\" }}");
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

        // Render History the way the Rust example's Debug output does.
        static string HistoryText(History? history)
        {
            if (history == null) return "KeepLast(1)";
            return history.Kind == HistoryKind.KeepAll ? "KeepAll" : $"KeepLast({history.Depth})";
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
