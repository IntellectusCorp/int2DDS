using System;
using System.IO;
using Int2Dds.Conditions;
using Int2Dds.Core;
using Int2Dds.Exceptions;

namespace QosProfileSubscriber
{
    // Companion to QosProfilePublisher. Loads QoS settings from an XML profile file
    // (RTI/OMG <qos_library> syntax) via the DDS_QOS_PROFILE environment variable and
    // subscribes to HelloWorld samples with the named profile applied.
    //
    // This example points DDS_QOS_PROFILE at the shared qos_profiles.xml (the same file
    // the Rust example uses) unless it is already set. Override the file with --xml.
    //
    // Usage: QosProfileSubscriber [--domain N] [--xml PATH]
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

            Console.WriteLine("=== QoS Profile Subscriber (C#) ===");
            Console.WriteLine($"Domain: {domain}");
            Console.WriteLine($"DDS_QOS_PROFILE: {Environment.GetEnvironmentVariable("DDS_QOS_PROFILE")}");
            Console.WriteLine($"Profile: {ProfilePath}\n");

            // Topic/Subscriber/DataReader are created from the named XML profile. The
            // profile defines no participant_qos, so the participant keeps spec defaults.
            using var dp = new DomainParticipant(domain, "qos_profile_subscriber");
            using var topic = dp.CreateTopicWithProfile<HelloWorld>("hello_world_topic", ProfilePath);
            using var sub = dp.CreateSubscriberWithProfile(ProfilePath);
            using var reader = sub.CreateDataReaderWithProfile(topic, ProfilePath);

            var qos = reader.GetQos();
            Console.WriteLine("DataReader QoS in effect:");
            Console.WriteLine($"  - Reliability: {qos.Reliability.Kind}");
            Console.WriteLine($"  - Durability:  {qos.Durability.Kind}");
            Console.WriteLine($"  - History:     {qos.History.Kind} (depth = {qos.History.Depth})");

            using var statusCond = reader.GetStatusCondition();
            statusCond.EnabledStatuses = StatusMask.SubscriptionMatched;

            Console.WriteLine("\nWaiting for publisher...");
            using var waitset = new WaitSet();
            waitset.Attach(statusCond);

            while (reader.MatchedWriters == 0)
            {
                try { waitset.Wait(TimeSpan.FromSeconds(1)); }
                catch (DdsTimeoutException) { /* timeout, retry */ }
            }

            Console.WriteLine($"Matched {reader.MatchedWriters} writer(s)\n");

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
