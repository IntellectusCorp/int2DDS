using System;
using System.Text;
using Int2Dds.Cdr;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.TypeInfo
{
    /// <summary>
    /// Fluent builder for constructing type information used when creating topics
    /// with runtime type definitions (via int2dds_type_info_* FFI functions).
    /// </summary>
    public sealed class TypeInfoBuilder : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;
        private bool _built;

        /// <summary>
        /// Creates a new TypeInfoBuilder for the given type name and extensibility.
        /// </summary>
        /// <param name="typeName">The fully qualified type name.</param>
        /// <param name="extensibility">The extensibility kind for the type.</param>
        public unsafe TypeInfoBuilder(string typeName, Extensibility extensibility)
        {
            var nameBytes = Encoding.UTF8.GetBytes(typeName + '\0');
            fixed (byte* p = nameBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_create(p, (int)extensibility, out _handle));
            }
        }

        /// <summary>
        /// Adds a scalar field to the type.
        /// </summary>
        /// <param name="fieldName">The field name.</param>
        /// <param name="fieldType">The field type constant (see <see cref="Int2Dds.Types.FieldType"/>).</param>
        /// <param name="isKey">Whether this field is part of the key.</param>
        /// <returns>This builder for chaining.</returns>
        public unsafe TypeInfoBuilder AddField(string fieldName, int fieldType, bool isKey = false)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var nameBytes = Encoding.UTF8.GetBytes(fieldName + '\0');
            fixed (byte* p = nameBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_add_field(_handle, p, fieldType, isKey ? 1 : 0));
            }
            return this;
        }

        /// <summary>
        /// Adds a sequence field to the type.
        /// </summary>
        /// <param name="fieldName">The field name.</param>
        /// <param name="elementType">The element type constant.</param>
        /// <param name="bound">Maximum sequence length (0 for unbounded).</param>
        /// <param name="isKey">Whether this field is part of the key.</param>
        /// <returns>This builder for chaining.</returns>
        public unsafe TypeInfoBuilder AddSequenceField(string fieldName, int elementType, uint bound = 0, bool isKey = false)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var nameBytes = Encoding.UTF8.GetBytes(fieldName + '\0');
            fixed (byte* p = nameBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_add_sequence_field(_handle, p, elementType, bound, isKey ? 1 : 0));
            }
            return this;
        }

        /// <summary>
        /// Adds a fixed-size array field to the type.
        /// </summary>
        /// <param name="fieldName">The field name.</param>
        /// <param name="elementType">The element type constant.</param>
        /// <param name="arraySize">The fixed array size.</param>
        /// <param name="isKey">Whether this field is part of the key.</param>
        /// <returns>This builder for chaining.</returns>
        public unsafe TypeInfoBuilder AddArrayField(string fieldName, int elementType, uint arraySize, bool isKey = false)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var nameBytes = Encoding.UTF8.GetBytes(fieldName + '\0');
            fixed (byte* p = nameBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_add_array_field(_handle, p, elementType, arraySize, isKey ? 1 : 0));
            }
            return this;
        }

        /// <summary>
        /// Adds a named type (struct/nested) field to the type.
        /// </summary>
        /// <param name="fieldName">The field name.</param>
        /// <param name="typeHashName">The type hash name of the referenced type.</param>
        /// <param name="isKey">Whether this field is part of the key.</param>
        /// <returns>This builder for chaining.</returns>
        public unsafe TypeInfoBuilder AddNamedTypeField(string fieldName, string typeHashName, bool isKey = false)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var nameBytes = Encoding.UTF8.GetBytes(fieldName + '\0');
            var typeBytes = Encoding.UTF8.GetBytes(typeHashName + '\0');
            fixed (byte* pName = nameBytes)
            fixed (byte* pType = typeBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_add_named_type_field(_handle, pName, pType, isKey ? 1 : 0));
            }
            return this;
        }

        /// <summary>
        /// Adds a sequence-of-named-type field (sequence whose elements are a nested type).
        /// </summary>
        public unsafe TypeInfoBuilder AddSequenceOfNamedField(string fieldName, string elementHashName, uint bound = 0, bool isKey = false)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var nameBytes = Encoding.UTF8.GetBytes(fieldName + '\0');
            var typeBytes = Encoding.UTF8.GetBytes(elementHashName + '\0');
            fixed (byte* pName = nameBytes)
            fixed (byte* pType = typeBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_add_sequence_of_named_field(_handle, pName, pType, bound, isKey ? 1 : 0));
            }
            return this;
        }

        /// <summary>
        /// Adds an array-of-named-type field (fixed array whose elements are a nested type).
        /// </summary>
        public unsafe TypeInfoBuilder AddArrayOfNamedField(string fieldName, string elementHashName, uint arraySize, bool isKey = false)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            var nameBytes = Encoding.UTF8.GetBytes(fieldName + '\0');
            var typeBytes = Encoding.UTF8.GetBytes(elementHashName + '\0');
            fixed (byte* pName = nameBytes)
            fixed (byte* pType = typeBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_add_array_of_named_field(_handle, pName, pType, arraySize, isKey ? 1 : 0));
            }
            return this;
        }

        /// <summary>
        /// Builds a standalone TypeObject (for local introspection / dynamic decoding).
        /// The builder remains usable afterwards.
        /// </summary>
        public Xtypes.DynamicTypeObject ToTypeObject()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_type_info_to_type_object(_handle, out IntPtr typeObj));
            return new Xtypes.DynamicTypeObject(typeObj);
        }

        /// <summary>
        /// Finalizes the builder and returns the native type info handle.
        /// The caller takes ownership of the handle (for passing to create_topic_with_type_info).
        /// After calling Build(), this builder must not be used further.
        /// </summary>
        /// <returns>The native type info handle.</returns>
        public IntPtr Build()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            if (_built)
                throw new InvalidOperationException("TypeInfoBuilder has already been built.");

            _built = true;
            IntPtr result = _handle;
            _handle = IntPtr.Zero;
            return result;
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            // Only destroy if Build() was not called (ownership was not transferred).
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.int2dds_type_info_destroy(_handle);
                _handle = IntPtr.Zero;
            }
        }
    }
}
