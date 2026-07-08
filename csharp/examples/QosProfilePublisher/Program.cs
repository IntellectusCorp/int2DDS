using System;
using System.IO;
using System.Threading;
using Int2Dds.Conditions;
using Int2Dds.Core;

namespace QosProfilePublisher
{
    // Loads QoS settings from an XML profile file and
    // publishes HelloWorld samples with the named profile applied. int2dds auto-loads
    // the profiles named by the DDS_QOS_PROFILE environment variable when the participant
    // factory is first created; the CreateWithProfile APIs then pick a profile by path.
    //
    // This example points DDS_QOS_PROFILE at the shared qos_profiles.xml (the same file
    // the Rust example uses) unless it is already set. Override the file with --xml.
    //
    // Usage: QosProfilePublisher [--domain N] [--xml PATH]
    internal static class Program
    {
        // "Library::Profile" path of the named profile to apply.
        private const string ProfilePath = "HelloWorldLibrary::ReliableProfile";

        private static void Main(string[] args)
        {
            int domain = IntArg(args, "--domain", 0);
            string xmlPath = StrArg(args, "--xml", DefaultXml());

            // The factory reads DDS_QOS_PROFILE when it is first created, so set it
            // before any DomainParticipant is constructed. A pre-set value wins.
            if (string.IsNullOrEmpty(Environment.GetEnvironmentVariable("DDS_QOS_PROFILE")))
                Environment.SetEnvironmentVariable("DDS_QOS_PROFILE", xmlPath);

            Console.WriteLine("=== QoS Profile Publisher (C#) ===");
            Console.WriteLine($"Domain: {domain}");
            Console.WriteLine($"DDS_QOS_PROFILE: {Environment.GetEnvironmentVariable("DDS_QOS_PROFILE")}");
            Console.WriteLine($"Profile: {ProfilePath}\n");

            // Topic/Publisher/DataWriter are created from the named XML profile. The
            // profile defines no participant_qos, so the participant keeps spec defaults.
            using var dp = new DomainParticipant(domain, "qos_profile_publisher");
            using var topic = dp.CreateTopicWithProfile<HelloWorld>("hello_world_topic", ProfilePath);
            using var pub = dp.CreatePublisherWithProfile(ProfilePath);
            using var writer = pub.CreateDataWriterWithProfile(topic, ProfilePath);

            var qos = writer.GetQos();
            Console.WriteLine("DataWriter QoS in effect:");
            Console.WriteLine($"  - Reliability: {qos.Reliability.Kind}");
            Console.WriteLine($"  - Durability:  {qos.Durability.Kind}");
            Console.WriteLine($"  - History:     {qos.History.Kind} (depth = {qos.History.Depth})");

            Console.WriteLine("\nWaiting for subscriber...");
            using var statusCondition = writer.GetStatusCondition();
            statusCondition.EnabledStatuses = StatusMask.PublicationMatched;
            using var waitset = new WaitSet();
            waitset.Attach(statusCondition);

            while (writer.MatchedReaders == 0)
            {
                try { waitset.Wait(TimeSpan.FromSeconds(1)); }
                catch { /* timeout, retry */ }
            }

            Console.WriteLine($"Matched {writer.MatchedReaders} reader(s)\n");

            for (uint i = 0; i < 20; i++)
            {
                var sample = new HelloWorld(i, $"Hello from QoS profile (C#)! ({i})");
                writer.Write(sample);
                Console.WriteLine($"Published: index={sample.Index}, message='{sample.Message}'");
                Thread.Sleep(500);
            }

            Console.WriteLine("Done publishing");
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
                var candidate = Path.Combine(dir.FullName, "dds", "examples", "qos_profile", "xml", "qos_profiles.xml");
                if (File.Exists(candidate))
                    return candidate;
                dir = dir.Parent;
            }
            return Path.Combine("..", "..", "dds", "examples", "qos_profile", "xml", "qos_profiles.xml");
        }
    }
}
