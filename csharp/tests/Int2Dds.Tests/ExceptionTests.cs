using System;
using Int2Dds.Core;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Xunit;

namespace Int2Dds.Tests
{
    public class ExceptionTests
    {
        [Fact]
        public void CheckReturn_Ok_DoesNotThrow()
        {
            ReturnCodeHelper.CheckReturn(ReturnCode.Ok);
        }

        [Fact]
        public void CheckReturn_Error_ThrowsDdsErrorException()
        {
            Assert.Throws<DdsErrorException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.Error));
        }

        [Fact]
        public void CheckReturn_Timeout_ThrowsDdsTimeoutException()
        {
            Assert.Throws<DdsTimeoutException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.Timeout));
        }

        [Fact]
        public void CheckReturn_Unsupported_ThrowsDdsUnsupportedException()
        {
            Assert.Throws<DdsUnsupportedException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.Unsupported));
        }

        [Fact]
        public void CheckReturn_InvalidArgument_ThrowsDdsInvalidArgumentException()
        {
            Assert.Throws<DdsInvalidArgumentException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.InvalidArgument));
        }

        [Fact]
        public void CheckReturn_AlreadyDeleted_ThrowsDdsAlreadyDeletedException()
        {
            Assert.Throws<DdsAlreadyDeletedException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.AlreadyDeleted));
        }

        [Fact]
        public void CheckReturn_NotEnabled_ThrowsDdsNotEnabledException()
        {
            Assert.Throws<DdsNotEnabledException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.NotEnabled));
        }

        [Fact]
        public void CheckReturn_ImmutablePolicy_ThrowsDdsImmutablePolicyException()
        {
            Assert.Throws<DdsImmutablePolicyException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.ImmutablePolicy));
        }

        [Fact]
        public void CheckReturn_InconsistentPolicy_ThrowsDdsInconsistentPolicyException()
        {
            Assert.Throws<DdsInconsistentPolicyException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.InconsistentPolicy));
        }

        [Fact]
        public void CheckReturn_PreconditionNotMet_ThrowsDdsPreconditionNotMetException()
        {
            Assert.Throws<DdsPreconditionNotMetException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.PreconditionNotMet));
        }

        [Fact]
        public void CheckReturn_OutOfResources_ThrowsDdsOutOfResourcesException()
        {
            Assert.Throws<DdsOutOfResourcesException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.OutOfResources));
        }

        [Fact]
        public void CheckReturn_IllegalOperation_ThrowsDdsIllegalOperationException()
        {
            Assert.Throws<DdsIllegalOperationException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.IllegalOperation));
        }

        [Fact]
        public void CheckReturn_NoData_ThrowsDdsNoDataException()
        {
            Assert.Throws<DdsNoDataException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.NoData));
        }

        [Fact]
        public void CheckReturn_NullPointer_ThrowsDdsNullPointerException()
        {
            Assert.Throws<DdsNullPointerException>(() => ReturnCodeHelper.CheckReturn(ReturnCode.NullPointer));
        }

        [Fact]
        public void CheckReturn_Unknown_ThrowsDdsException()
        {
            var ex = Assert.Throws<DdsException>(() => ReturnCodeHelper.CheckReturn(999));
            Assert.Equal(999, ex.Code);
        }

        [Fact]
        public void CheckReturnOrNoData_Ok_ReturnsTrue()
        {
            Assert.True(ReturnCodeHelper.CheckReturnOrNoData(ReturnCode.Ok));
        }

        [Fact]
        public void CheckReturnOrNoData_NoData_ReturnsFalse()
        {
            Assert.False(ReturnCodeHelper.CheckReturnOrNoData(ReturnCode.NoData));
        }

        [Fact]
        public void CheckReturnOrNoData_Error_Throws()
        {
            Assert.Throws<DdsErrorException>(() => ReturnCodeHelper.CheckReturnOrNoData(ReturnCode.Error));
        }

        [Fact]
        public void CreateSubscriberWithMissingProfile_IncludesReasonInMessage()
        {
            using var dp = new DomainParticipant(90);

            var ex = Assert.Throws<DdsErrorException>(() =>
                dp.CreateSubscriberWithProfile("NoSuchLib::NoSuchProfile"));

            Assert.Contains("QoS profile not found", ex.Message);
        }

        [Fact]
        public void DdsException_HasCorrectCode()
        {
            var ex = new DdsTimeoutException();
            Assert.Equal(ReturnCode.Timeout, ex.Code);
            Assert.IsAssignableFrom<DdsException>(ex);
            Assert.IsAssignableFrom<Exception>(ex);
        }
    }
}
