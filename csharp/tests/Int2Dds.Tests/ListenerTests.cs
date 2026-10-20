using System;
using System.Diagnostics;
using System.Threading;
using Int2Dds.Core;
using Int2Dds.Listeners;
using Int2Dds.Tests.Helpers;
using Xunit;

namespace Int2Dds.Tests
{
    public class ListenerTests
    {
        private class DataAvailableListener : DataReaderListenerBase
        {
            private volatile bool _isDataAvailableCalled;

            public bool IsDataAvailableCalled => _isDataAvailableCalled;

            public override void OnDataAvailable(object reader)
            {
                _isDataAvailableCalled = true;
            }
        }

        /// <summary>A listener passed without a status mask receives OnDataAvailable.</summary>
        [Fact]
        public void OnDataAvailableIsCalledWithDefaultStatusMask()
        {
            using (var dp = new DomainParticipant(73))
            using (var topic = dp.CreateTopic<HelloWorld>("ListenerDefaultMaskCs"))
            {
                var writer = dp.CreatePublisher().CreateDataWriter(topic);
                var listener = new DataAvailableListener();
                var reader = dp.CreateSubscriber().CreateDataReader(topic, null, listener);

                var sw = Stopwatch.StartNew();
                while (reader.GetSubscriptionMatchedStatus().currentCount == 0)
                {
                    Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), "no match within 5s");
                    Thread.Sleep(10);
                }

                writer.Write(new HelloWorld(1, "a"));

                sw.Restart();
                while (!listener.IsDataAvailableCalled)
                {
                    Assert.True(sw.Elapsed < TimeSpan.FromSeconds(5), "OnDataAvailable not called within 5s");
                    Thread.Sleep(10);
                }
            }
        }
    }
}
