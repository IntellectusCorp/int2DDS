namespace Int2Dds.Core
{
    /// <summary>
    /// A data sample received from a DataReader.
    /// </summary>
    /// <typeparam name="T">The DDS data type.</typeparam>
    public readonly struct Sample<T> where T : class
    {
        /// <summary>The deserialized data, or null if not valid data (e.g., dispose/unregister notification).</summary>
        public T? Data { get; }

        /// <summary>True if this sample contains valid data; false for lifecycle notifications.</summary>
        public bool ValidData { get; }

        public Sample(T? data, bool validData)
        {
            Data = data;
            ValidData = validData;
        }
    }
}
