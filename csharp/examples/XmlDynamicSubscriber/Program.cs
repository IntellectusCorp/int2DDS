using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
using Int2Dds.Core;
using Int2Dds.Xtypes;

namespace XmlDynamicSubscriber
{
    // Loads a type defined in XML at runtime — the same XmlTypeRegistry workflow as the
    // Rust and Python examples — and subscribes to it without any compile-time IDL. The
    // companion XmlDynamicPublisher loads the same XML and publishes it.
    //
    // Usage: XmlDynamicSubscriber [--domain N] [--xml PATH] [--type NAME]
    internal static class Program
    {
        private static int Main(string[] args)
        {
            int domain = IntArg(args, "--domain", 0);
            string xmlPath = StrArg(args, "--xml", DefaultXml());
            string typeName = StrArg(args, "--type", "SensorData");

            Console.WriteLine("=== XML Dynamic Type Subscriber (C#) ===");
            Console.WriteLine($"Domain: {domain}");
            Console.WriteLine($"XML : {xmlPath}");
            Console.WriteLine($"Type: {typeName}\n");

            using var registry = XmlTypeRegistry.FromFile(xmlPath);
            using var support = registry.GetTypeSupport(typeName);
            using var dp = new DomainParticipant(domain, "xml_dynamic_subscriber");

            using var topic = dp.CreateTopicDynamic("SensorTopic", support);
            using var reader = dp.CreateSubscriber().CreateDataReaderDynamic(topic, support);

            Console.WriteLine("Waiting for a publisher and samples...");
            bool receivedAny = false;
            var sw = Stopwatch.StartNew();
            while (sw.Elapsed.TotalSeconds < 25.0)
            {
                var sample = reader.Take();
                if (sample != null)
                {
                    receivedAny = true;
                    using (sample)
                        Console.WriteLine($"[RECV] sensor_id={sample.GetI32("sensor_id")} " +
                            $"temperature={sample.GetF64("temperature"):F1} humidity={sample.GetF64("humidity"):F1}");
                    sw.Restart();
                }
                else
                {
                    Thread.Sleep(50);
                }
            }

            if (!receivedAny)
            {
                Console.Error.WriteLine("no sample received");
                return 1;
            }
            return 0;
        }

        private static int IntArg(string[] args, string name, int fallback)
        {
            for (int i = 0; i < args.Length - 1; i++)
                if (args[i] == name && int.TryParse(args[i + 1], out int v))
                    return v;
            return fallback;
        }

        private static string StrArg(string[] args, string name, string fallback)
        {
            for (int i = 0; i < args.Length - 1; i++)
                if (args[i] == name)
                    return args[i + 1];
            return fallback;
        }

        private static string DefaultXml()
        {
            var dir = new DirectoryInfo(AppContext.BaseDirectory);
            while (dir != null)
            {
                var candidate = Path.Combine(dir.FullName, "dds", "examples", "xtypes", "sensor_data.xml");
                if (File.Exists(candidate))
                    return candidate;
                dir = dir.Parent;
            }
            return Path.Combine("..", "..", "dds", "examples", "xtypes", "sensor_data.xml");
        }
    }
}
