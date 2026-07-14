using System;
using Int2Dds.Interop;

namespace Int2Dds.Exceptions
{
    /// <summary>
    /// Base exception for DDS operations.
    /// </summary>
    public class DdsException : Exception
    {
        public int Code { get; }

        public DdsException(string message, int code) : base(message)
        {
            Code = code;
        }

        public DdsException(string message, int code, Exception innerException) : base(message, innerException)
        {
            Code = code;
        }
    }

    public class DdsErrorException : DdsException
    {
        public DdsErrorException() : base("DDS operation failed.", ReturnCode.Error) { }

        public DdsErrorException(string message)
            : base(string.IsNullOrEmpty(message) ? "DDS operation failed." : message, ReturnCode.Error) { }
    }

    public class DdsTimeoutException : DdsException
    {
        public DdsTimeoutException() : base("DDS operation timed out.", ReturnCode.Timeout) { }
    }

    public class DdsUnsupportedException : DdsException
    {
        public DdsUnsupportedException() : base("DDS operation not supported.", ReturnCode.Unsupported) { }
    }

    public class DdsInvalidArgumentException : DdsException
    {
        public DdsInvalidArgumentException() : base("DDS invalid argument.", ReturnCode.InvalidArgument) { }
    }

    public class DdsAlreadyDeletedException : DdsException
    {
        public DdsAlreadyDeletedException() : base("DDS entity already deleted.", ReturnCode.AlreadyDeleted) { }
    }

    public class DdsNotEnabledException : DdsException
    {
        public DdsNotEnabledException() : base("DDS entity not enabled.", ReturnCode.NotEnabled) { }
    }

    public class DdsImmutablePolicyException : DdsException
    {
        public DdsImmutablePolicyException() : base("DDS immutable policy cannot be changed.", ReturnCode.ImmutablePolicy) { }
    }

    public class DdsInconsistentPolicyException : DdsException
    {
        public DdsInconsistentPolicyException() : base("DDS inconsistent policy.", ReturnCode.InconsistentPolicy) { }
    }

    public class DdsPreconditionNotMetException : DdsException
    {
        public DdsPreconditionNotMetException() : base("DDS precondition not met.", ReturnCode.PreconditionNotMet) { }
    }

    public class DdsOutOfResourcesException : DdsException
    {
        public DdsOutOfResourcesException() : base("DDS out of resources.", ReturnCode.OutOfResources) { }
    }

    public class DdsIllegalOperationException : DdsException
    {
        public DdsIllegalOperationException() : base("DDS illegal operation.", ReturnCode.IllegalOperation) { }
    }

    public class DdsNoDataException : DdsException
    {
        public DdsNoDataException() : base("DDS no data available.", ReturnCode.NoData) { }
    }

    public class DdsNullPointerException : DdsException
    {
        public DdsNullPointerException() : base("DDS null pointer.", ReturnCode.NullPointer) { }
    }

    public class DdsBufferTooSmallException : DdsException
    {
        public DdsBufferTooSmallException() : base("DDS buffer too small.", ReturnCode.BufferTooSmall) { }
    }

    /// <summary>
    /// Helper to check FFI return codes and throw the appropriate exception.
    /// </summary>
    internal static class ReturnCodeHelper
    {
        internal static void CheckReturn(int ret)
        {
            if (ret == ReturnCode.Ok)
                return;

            throw ret switch
            {
                ReturnCode.Error => new DdsErrorException(Interop.NativeLastError.GetMessage()),
                ReturnCode.Timeout => new DdsTimeoutException(),
                ReturnCode.Unsupported => new DdsUnsupportedException(),
                ReturnCode.InvalidArgument => new DdsInvalidArgumentException(),
                ReturnCode.AlreadyDeleted => new DdsAlreadyDeletedException(),
                ReturnCode.NotEnabled => new DdsNotEnabledException(),
                ReturnCode.ImmutablePolicy => new DdsImmutablePolicyException(),
                ReturnCode.InconsistentPolicy => new DdsInconsistentPolicyException(),
                ReturnCode.PreconditionNotMet => new DdsPreconditionNotMetException(),
                ReturnCode.OutOfResources => new DdsOutOfResourcesException(),
                ReturnCode.IllegalOperation => new DdsIllegalOperationException(),
                ReturnCode.NoData => new DdsNoDataException(),
                ReturnCode.NullPointer => new DdsNullPointerException(),
                ReturnCode.BufferTooSmall => new DdsBufferTooSmallException(),
                _ => new DdsException($"DDS operation failed with unknown code {ret}.", ret),
            };
        }

        /// <summary>
        /// Returns true if ret == OK, false if ret == NO_DATA, throws for other errors.
        /// </summary>
        internal static bool CheckReturnOrNoData(int ret)
        {
            if (ret == ReturnCode.Ok)
                return true;
            if (ret == ReturnCode.NoData)
                return false;

            CheckReturn(ret);
            return false; // unreachable
        }
    }
}
