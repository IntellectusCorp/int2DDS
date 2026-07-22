using Int2Dds.Core;
using Int2Dds.Qos;
using Xunit;

namespace Int2Dds.Tests
{
    public class ParticipantQosTests
    {
        [Fact]
        public void SetQos_ThenGetQos_PreservesProperty()
        {
            using (var dp = new DomainParticipant(0))
            {
                var qos = new ParticipantQos { Property = new Property() };
                qos.Property.Add("vendor.us.int2.participant_qos", "live");
                dp.SetQos(qos);

                var got = dp.GetQos();
                Assert.NotNull(got.Property);
                Assert.Contains(
                    got.Property.Entries,
                    e => e.Name == "vendor.us.int2.participant_qos" && e.Value == "live");
            }
        }
    }
}
