using System;
using System.Threading;
using Int2Dds.Conditions;
using Int2Dds.Core;
using Int2Dds.Qos;

namespace HelloWorldPub
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

            using var dp = new DomainParticipant(domainId: domainId, name: "CSharpPublisher");

            using var topic = dp.CreateTopic<HelloWorld>(TopicName);

            using var pub = dp.CreatePublisher();
            var writerQos = new DataWriterQos
            {
                Reliability = new Reliability(
                    reliable ? ReliabilityKind.Reliable : ReliabilityKind.BestEffort,
                    reliable ? TimeSpan.FromMilliseconds(100) : (TimeSpan?)null),
            };
            using var writer = pub.CreateDataWriter(topic, writerQos);

            var wqos = writer.GetQos();
            Console.WriteLine($"[publisher INFO] domain_id: {domainId}, topic: {TopicName}");
            Console.WriteLine($"[publisher qos] reliability: {wqos.Reliability?.Kind}, durability: {wqos.Durability?.Kind}, history: {HistoryText(wqos.History)}");

            // Wait for subscriber to connect
            using var statusCondition = writer.GetStatusCondition();
            statusCondition.EnabledStatuses = StatusMask.PublicationMatched;
            using var waitset = new WaitSet();
            waitset.Attach(statusCondition);

            while (writer.MatchedReaders == 0 && !stop.IsSet)
            {
                try { waitset.Wait(TimeSpan.FromSeconds(1)); }
                catch { /* timeout, retry */ }
            }

            if (stop.IsSet)
            {
                return;
            }

            Console.WriteLine("Subscriber matched!");

            // Publish samples until Ctrl-C
            uint i = 1;
            while (!stop.IsSet)
            {
                var sample = new HelloWorld { Index = i, Message = $"[C#]HelloWorld_d{domainId}" };
                writer.Write(sample);
                Console.WriteLine($"Published HelloWorld {{ index: {sample.Index}, message: \"{sample.Message}\" }}");
                stop.Wait(TimeSpan.FromSeconds(1));
                i++;
            }
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
