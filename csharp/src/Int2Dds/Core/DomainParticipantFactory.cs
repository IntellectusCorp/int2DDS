using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Core;

/// <summary>
/// Singleton factory for creating DomainParticipants.
/// Wraps the native DomainParticipantFactory handle.
/// </summary>
public sealed class DomainParticipantFactory
{
    private static readonly Lazy<DomainParticipantFactory> _instance = new(() =>
    {
        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_domain_participant_factory_get_instance(out var handle));
        return new DomainParticipantFactory(handle);
    });

    /// <summary>
    /// Gets the singleton DomainParticipantFactory instance.
    /// </summary>
    public static DomainParticipantFactory Instance => _instance.Value;

    internal nint Handle { get; }

    private DomainParticipantFactory(nint handle)
    {
        Handle = handle;
    }
}
