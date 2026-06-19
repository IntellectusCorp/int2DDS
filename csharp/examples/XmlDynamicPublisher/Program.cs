using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
using Int2Dds.Core;
using Int2Dds.Xtypes;

namespace XmlDynamicPublisher
{
    // Loads a type defined in XML at runtime — the same XmlTypeRegistry workflow as the
    // Rust and Python examples — and publishes it without any compile-time IDL. The
    // companion XmlDynamicSubscriber loads the same XML and receives it.
    //
    // Usage: XmlDynamicPublisher [--domain N] [--xml PATH] [--type NAME]
    internal static class Program
    {
        private static int Main(string[] args)
        {
            int domain = IntArg(args, "--domain", 0);
            string xmlPath = StrArg(args, "--xml", DefaultXml());
            string typeName = StrArg(args, "--type", "SensorData");

            Console.WriteLine("=== XML Dynamic Type Publisher (C#) ===");
            Console.WriteLine($"Domain: {domain}");
            Console.WriteLine($"XML : {xmlPath}");
            Console.WriteLine($"Type: {typeName}\n");

            using var registry = XmlTypeRegistry.FromFile(xmlPath);
            using var support = registry.GetTypeSupport(typeName);
            using var dp = new DomainParticipant(domain, "xml_dynamic_publisher");

            using var topic = dp.CreateTopicDynamic("SensorTopic", support);
            using var writer = dp.CreatePublisher().CreateDataWriterDynamic(topic, support);

            Console.WriteLine("Waiting for a subscriber to match...");
            for (int i = 0; i < 400 && writer.PublicationMatchedCount() == 0; i++)
                Thread.Sleep(50);

            var sw = Stopwatch.StartNew();
            int n = 0;
            while (sw.Elapsed.TotalSeconds < 15.0)
            {
                using (var data = support.CreateData())
                {
                    data.SetI32("sensor_id", 42);
                    data.SetF64("temperature", 23.5);
                    data.SetF64("humidity", 48.0);
                    writer.Write(data);
                }
                Console.WriteLine($"[SEND] #{++n} sensor_id=42 temperature=23.5 humidity=48.0");
                Thread.Sleep(300);
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
