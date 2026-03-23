namespace Int2Dds.Core;

/// <summary>
/// A data sample received from a DataReader.
/// </summary>
/// <typeparam name="T">The DDS data type.</typeparam>
/// <param name="Data">The deserialized data, or null if not valid data (e.g., dispose/unregister notification).</param>
/// <param name="ValidData">True if this sample contains valid data; false for lifecycle notifications.</param>
public readonly record struct Sample<T>(T? Data, bool ValidData);
