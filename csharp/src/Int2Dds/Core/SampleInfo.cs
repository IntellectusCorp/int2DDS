using System;

namespace Int2Dds.Core
{
    /// <summary>
    /// Metadata associated with a received data sample.
    /// </summary>
    public readonly struct SampleInfo
    {
        /// <summary>Source timestamp (seconds component).</summary>
        public int SourceTimestampSec { get; }

        /// <summary>Source timestamp (nanoseconds component).</summary>
        public uint SourceTimestampNanosec { get; }

        /// <summary>Sample state bitmask (Read / NotRead).</summary>
        public uint SampleState { get; }

        /// <summary>View state bitmask (New / NotNew).</summary>
        public uint ViewState { get; }

        /// <summary>Instance state bitmask (Alive / NotAliveDisposed / NotAliveNoWriters).</summary>
        public uint InstanceState { get; }

        /// <summary>Handle identifying the data instance.</summary>
        public InstanceHandle InstanceHandle { get; }

        /// <summary>Handle identifying the publication that wrote this sample.</summary>
        public InstanceHandle PublicationHandle { get; }

        /// <summary>Number of times the instance has been disposed.</summary>
        public int DisposedGenerationCount { get; }

        /// <summary>Number of times the instance has had zero writers.</summary>
        public int NoWritersGenerationCount { get; }

        /// <summary>Rank of this sample within the instance.</summary>
        public int SampleRank { get; }

        /// <summary>Generation rank of this sample.</summary>
        public int GenerationRank { get; }

        /// <summary>Absolute generation rank of this sample.</summary>
        public int AbsoluteGenerationRank { get; }

        /// <summary>True if this sample contains valid data.</summary>
        public bool ValidData { get; }

        public SampleInfo(
            int sourceTimestampSec,
            uint sourceTimestampNanosec,
            uint sampleState,
            uint viewState,
            uint instanceState,
            InstanceHandle instanceHandle,
            InstanceHandle publicationHandle,
            int disposedGenerationCount,
            int noWritersGenerationCount,
            int sampleRank,
            int generationRank,
            int absoluteGenerationRank,
            bool validData)
        {
            SourceTimestampSec = sourceTimestampSec;
            SourceTimestampNanosec = sourceTimestampNanosec;
            SampleState = sampleState;
            ViewState = viewState;
            InstanceState = instanceState;
            InstanceHandle = instanceHandle;
            PublicationHandle = publicationHandle;
            DisposedGenerationCount = disposedGenerationCount;
            NoWritersGenerationCount = noWritersGenerationCount;
            SampleRank = sampleRank;
            GenerationRank = generationRank;
            AbsoluteGenerationRank = absoluteGenerationRank;
            ValidData = validData;
        }

        /// <summary>
        /// Reconstructs the source timestamp as a DateTimeOffset.
        /// </summary>
        public DateTimeOffset SourceTimestamp =>
            DateTimeOffset.UnixEpoch.AddSeconds(SourceTimestampSec).AddTicks(SourceTimestampNanosec / 100);
    }
}
