using Int2Dds.Conditions;
using Int2Dds.Core;

namespace HelloWorldPublisher;

class Program
{
    static void Main(string[] args)
    {
        Console.WriteLine("=== HelloWorld Publisher (C#) ===");

        using var dp = new DomainParticipant(domainId: 0, name: "CSharpPublisher");
        Console.WriteLine($"Created participant on domain {dp.DomainId}");

        using var topic = dp.CreateTopic<HelloWorld>("hello_world_topic");
        Console.WriteLine($"Created topic: {topic.Name} ({topic.TypeName})");

        using var pub = dp.CreatePublisher();
        using var writer = pub.CreateDataWriter(topic);
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
        for (uint i = 0; i < 10; i++)
        {
            var sample = new HelloWorld(i, $"Hello from C#! ({i})");
            writer.Write(sample);
            Console.WriteLine($"Published: index={sample.Index}, message='{sample.Message}'");
            Thread.Sleep(500);
        }

        Console.WriteLine("Done publishing");
    }
}
