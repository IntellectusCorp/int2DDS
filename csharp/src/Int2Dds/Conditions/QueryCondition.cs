using System;
using System.Runtime.InteropServices;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Conditions
{
    /// <summary>
    /// A <see cref="ReadCondition"/> that additionally applies a SQL-92 content
    /// filter. Content filtering requires the topic to carry field descriptors
    /// (the same requirement as ContentFilteredTopic); on a plain raw topic the
    /// filtered read raises an error instead of ignoring the filter.
    /// </summary>
    public sealed class QueryCondition : ReadCondition
    {
        internal QueryCondition(IntPtr handle) : base(handle)
        {
        }

        /// <summary>
        /// Replaces the parameters bound to the query expression.
        /// </summary>
        public void SetQueryParameters(params string[] parameters)
        {
            parameters ??= new string[0];
            unsafe
            {
                var byteArrays = new byte[parameters.Length][];
                var pins = new GCHandle[parameters.Length];
                for (int i = 0; i < parameters.Length; i++)
                {
                    byteArrays[i] = Encoding.UTF8.GetBytes(parameters[i] + '\0');
                    pins[i] = GCHandle.Alloc(byteArrays[i], GCHandleType.Pinned);
                }
                try
                {
                    var ptrs = new byte*[parameters.Length == 0 ? 1 : parameters.Length];
                    for (int i = 0; i < parameters.Length; i++)
                        ptrs[i] = (byte*)pins[i].AddrOfPinnedObject();
                    fixed (byte** pParams = ptrs)
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_querycondition_set_query_parameters(
                                Handle, pParams, (UIntPtr)parameters.Length));
                    }
                }
                finally
                {
                    foreach (var pin in pins)
                        if (pin.IsAllocated) pin.Free();
                }
            }
        }
    }
}
