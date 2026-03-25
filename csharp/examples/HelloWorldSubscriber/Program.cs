using System;
using Int2Dds.Conditions;
using Int2Dds.Core;
using Int2Dds.Exceptions;

namespace HelloWorldSubscriber
{
    class Program
    {
        static void Main(string[] args)
        {
            Console.WriteLine("=== HelloWorld Subscriber (C#) ===");

            using var dp = new DomainParticipant(domainId: 0, name: "CSharpSubscriber");
            Console.WriteLine($"Created participant on domain {dp.DomainId}");

            using var topic = dp.CreateTopic<HelloWorld>("hello_world_topic");
            Console.WriteLine($"Created topic: {topic.Name} ({topic.TypeName})");

            using var sub = dp.CreateSubscriber();
            using var reader = sub.CreateDataReader(topic);
            Console.WriteLine("Created subscriber and data reader");

            // Get StatusCondition and configure for discovery phase
            using var statusCond = reader.GetStatusCondition();
            statusCond.EnabledStatuses = StatusMask.SubscriptionMatched;

            // Wait for publisher to connect
            Console.WriteLine("Waiting for publisher...");
            using var waitset = new WaitSet();
            waitset.Attach(statusCond);

            while (reader.MatchedWriters == 0)
            {
                try { waitset.Wait(TimeSpan.FromSeconds(1)); }
                catch (DdsTimeoutException) { /* timeout, retry */ }
            }

            Console.WriteLine($"Matched {reader.MatchedWriters} writer(s)");

            // Switch to DATA_AVAILABLE for data reception
            statusCond.EnabledStatuses = StatusMask.DataAvailable;

            // Receive samples
            Console.WriteLine("Waiting for data...");
            int samplesReceived = 0;
            int timeoutCount = 0;

            while (timeoutCount < 3)
            {
                // Check for data that may have arrived already
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
