using System;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Xtypes
{
    /// <summary>
    /// An entire participant tree built from a <c>&lt;domain_participant_library&gt;</c>
    /// XML declaration via <see cref="XmlConfig.CreateParticipantFromConfig"/>:
    /// participant + publishers/subscribers + datawriters/datareaders + topics,
    /// with QoS taken from the profile. Endpoints carry <see cref="DynamicData"/> and
    /// are addressed by their XML name (<c>"publisher::writer"</c> /
    /// <c>"subscriber::reader"</c>).
    /// </summary>
    public sealed class ConfiguredParticipant : IDisposable
    {
        private IntPtr _handle;

        internal ConfiguredParticipant(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>Get the datawriter declared as <c>"&lt;publisher&gt;::&lt;writer&gt;"</c>.</summary>
        public unsafe DynamicTypeWriter DataWriter(string name)
        {
            var nb = NativeString.ToCStr(name);
            fixed (byte* p = nb)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_configured_participant_get_datawriter(_handle, p, out IntPtr h));
                return new DynamicTypeWriter(h);
            }
        }

        /// <summary>Get the datareader declared as <c>"&lt;subscriber&gt;::&lt;reader&gt;"</c>.</summary>
        public unsafe DynamicTypeReader DataReader(string name)
        {
            var nb = NativeString.ToCStr(name);
            fixed (byte* p = nb)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_configured_participant_get_datareader(_handle, p, out IntPtr h));
                return new DynamicTypeReader(h);
            }
        }

        /// <summary>
        /// Tear down the configured tree (participant + owned entities).
        /// Destroy any datawriter/datareader handles obtained via <see cref="DataWriter"/> /
        /// <see cref="DataReader"/> BEFORE disposing this — tearing down the tree deletes
        /// the participant's contained entities, so using those handles afterwards is undefined.
        /// </summary>
        public void Dispose()
        {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        /// <summary>Safety net for callers who forget <see cref="Dispose"/>.</summary>
        ~ConfiguredParticipant() => Dispose(false);

        private void Dispose(bool disposing)
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_configured_participant_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
