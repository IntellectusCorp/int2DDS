"""
Topic - associates a name with a data type.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Generic, TypeVar

from int2dds._ffi import ffi, lib
from int2dds.cdr.writer import Extensibility
from int2dds.core.conditions import StatusCondition
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.participant import DomainParticipant
    from int2dds.core.qos import TopicQos
    from int2dds.types.base import DdsType

T = TypeVar("T", bound="DdsType")


def _build_cstr_array(strings: list[str]):
    """Build a ``char *[]`` from a list of str; returns (array, [buffers]).

    The buffer list must be kept alive by the caller for the duration of the call.
    """
    bufs = [ffi.new("char[]", s.encode()) for s in strings]
    arr = ffi.new("char *[]", bufs) if bufs else ffi.NULL
    return arr, bufs


def _build_nested_type_info(cls):
    """Build a native Int2DdsTypeInfo for a nested generated class (struct or enum),
    dispatching on the descriptor the generator emitted. Returns a handle the caller owns.

    - Enum: ``_dds_enum_info = (bit_bound, ((name, value, is_default), ...))``
    - Struct: recurse over ``_dds_type_info_fields``.

    Enum literal names come from the descriptor (canonical PascalCase), so the built
    TypeObject byte-matches the Rust derive regardless of the Python member naming.
    """
    enum_info = getattr(cls, "_dds_enum_info", None)
    if enum_info is not None:
        bit_bound, literals = enum_info
        name_c = ffi.new("char[]", getattr(cls, "_dds_type_name", cls.__name__).encode())
        ti_ptr = ffi.new("Int2DdsTypeInfo **")
        check_ret(lib.int2dds_type_info_create_enum(name_c, bit_bound, ti_ptr))
        ti = ti_ptr[0]
        try:
            for lit_name, value, is_default in literals:
                lname_c = ffi.new("char[]", lit_name.encode())
                check_ret(
                    lib.int2dds_type_info_add_enum_literal(
                        ti, lname_c, value, 1 if is_default else 0
                    )
                )
        except Exception:
            lib.int2dds_type_info_destroy(ti)
            raise
        return ti

    nested_ext = getattr(cls, "_extensibility", Extensibility(lib.int2dds_default_extensibility()))
    return _build_type_info(
        getattr(cls, "_dds_type_name", cls.__name__),
        nested_ext,
        getattr(cls, "_dds_type_info_fields", []),
    )


def _build_type_info(type_name: str, extensibility: Extensibility, fields: list):
    """Build a native Int2DdsTypeInfo from generated ``_dds_type_info_fields`` metadata.

    Each entry is ``(op, name, type_const, size, flags)`` where ``op`` selects the
    ``int2dds_type_info_add_*`` call. The returned handle is owned by the caller and must be
    freed with ``int2dds_type_info_destroy`` after ``create_topic_with_type_info``.
    """
    name_c = ffi.new("char[]", type_name.encode())
    ti_ptr = ffi.new("Int2DdsTypeInfo **")
    check_ret(lib.int2dds_type_info_create(name_c, int(extensibility), ti_ptr))
    ti = ti_ptr[0]
    try:
        for op, field_name, type_const, size, flags in fields:
            fname_c = ffi.new("char[]", field_name.encode())
            if op == "field":
                check_ret(lib.int2dds_type_info_add_field(ti, fname_c, type_const, flags))
            elif op == "string":
                check_ret(lib.int2dds_type_info_add_string_field(ti, fname_c, size, flags))
            elif op == "wstring":
                check_ret(lib.int2dds_type_info_add_wstring_field(ti, fname_c, size, flags))
            elif op == "seq":
                check_ret(
                    lib.int2dds_type_info_add_sequence_field(ti, fname_c, type_const, size, flags)
                )
            elif op == "arr":
                check_ret(
                    lib.int2dds_type_info_add_array_field(ti, fname_c, type_const, size, flags)
                )
            elif op == "nested":
                # `type_const` holds the nested generated class (struct/enum/bitmask). Build its
                # own type_info and reference it by content-hash so composite keys resolve.
                nested_ti = _build_nested_type_info(type_const)
                try:
                    check_ret(
                        lib.int2dds_type_info_add_nested_field(ti, fname_c, nested_ti, flags)
                    )
                finally:
                    lib.int2dds_type_info_destroy(nested_ti)
            elif op == "seq_nested":
                # `type_const` holds the element class; `size` is the sequence bound.
                elem_ti = _build_nested_type_info(type_const)
                try:
                    check_ret(
                        lib.int2dds_type_info_add_sequence_of_nested_field(
                            ti, fname_c, elem_ti, size, flags
                        )
                    )
                finally:
                    lib.int2dds_type_info_destroy(elem_ti)
            elif op == "arr_nested":
                # `type_const` holds the element class; `size` is the array length.
                elem_ti = _build_nested_type_info(type_const)
                try:
                    check_ret(
                        lib.int2dds_type_info_add_array_of_nested_field(
                            ti, fname_c, elem_ti, size, flags
                        )
                    )
                finally:
                    lib.int2dds_type_info_destroy(elem_ti)
    except Exception:
        lib.int2dds_type_info_destroy(ti)
        raise
    return ti


class Topic(Generic[T]):
    """
    Topic - associates a name with a data type for publish/subscribe.

    Topics are created through DomainParticipant.create_topic().

    Attributes:
        name: The topic name
        type_name: The DDS type name
        type_class: The Python type class for serialization
    """

    __slots__ = ("_handle", "_participant", "_name", "_type_name", "_type_class", "_closed")

    def __init__(
        self,
        participant: DomainParticipant,
        topic_name: str,
        type_class: type[T],
        qos: TopicQos | None = None,
    ) -> None:
        self._participant = participant
        self._name = topic_name
        self._type_class = type_class
        self._closed = False

        # Get type metadata from the type class
        self._type_name: str = getattr(type_class, "_dds_type_name", type_class.__name__)
        extensibility: Extensibility = getattr(
            type_class, "_extensibility", Extensibility(lib.int2dds_default_extensibility())
        )
        has_key: bool = getattr(type_class, "_has_key", False)

        topic_name_c = ffi.new("char[]", topic_name.encode())
        type_name_c = ffi.new("char[]", self._type_name.encode())

        # Create Topic QoS if provided
        qos_ptr = ffi.NULL
        qos_handle = None
        if qos is not None:
            qos_handle_ptr = ffi.new("Int2DdsTopicQos **")
            check_ret(lib.int2dds_topic_qos_create_default(qos_handle_ptr))
            qos_handle = qos_handle_ptr[0]
            if qos.reliability is not None:
                check_ret(lib.int2dds_topic_qos_set_reliability(
                    qos_handle, qos.reliability._kind_int, qos.reliability._max_blocking_time_ns))
            if qos.durability is not None:
                check_ret(lib.int2dds_topic_qos_set_durability(
                    qos_handle, qos.durability._kind_int))
            if qos.history is not None:
                check_ret(lib.int2dds_topic_qos_set_history(
                    qos_handle, qos.history._kind_int, qos.history.depth))
            if qos.deadline is not None:
                check_ret(lib.int2dds_topic_qos_set_deadline(
                    qos_handle, qos.deadline._period_ns))
            if qos.liveliness is not None:
                check_ret(lib.int2dds_topic_qos_set_liveliness(
                    qos_handle, qos.liveliness._kind_int, qos.liveliness._lease_duration_ns))
            if qos.destination_order is not None:
                check_ret(lib.int2dds_topic_qos_set_destination_order(
                    qos_handle, qos.destination_order._kind_int))
            if qos.resource_limits is not None:
                check_ret(lib.int2dds_topic_qos_set_resource_limits(
                    qos_handle,
                    qos.resource_limits.max_samples,
                    qos.resource_limits.max_instances,
                    qos.resource_limits.max_samples_per_instance))
            if qos.transport_priority is not None:
                check_ret(lib.int2dds_topic_qos_set_transport_priority(
                    qos_handle, qos.transport_priority.value))
            if qos.lifespan is not None:
                check_ret(lib.int2dds_topic_qos_set_lifespan(
                    qos_handle, qos.lifespan._duration_ns))
            if qos.ownership is not None:
                check_ret(lib.int2dds_topic_qos_set_ownership(
                    qos_handle, qos.ownership._kind_int))
            if qos.data_representation is not None:
                check_ret(lib.int2dds_topic_qos_set_data_representation(
                    qos_handle, qos.data_representation._kind_int))
            qos_ptr = qos_handle

        topic_ptr = ffi.new("Int2DdsTopic **")

        # Extract key field metadata from type class for compute_key() support
        key_field_indices = []
        key_field_types = []
        _KEY_TYPE_MAP = {
            "str": 0, "string": 0,
            "int": 1, "i32": 1, "int32": 1,
            "uint": 2, "u32": 2, "uint32": 2,
            "i16": 3, "int16": 3,
            "u16": 4, "uint16": 4,
            "i64": 5, "int64": 5,
            "u64": 6, "uint64": 6,
            "i8": 7, "int8": 7,
            "u8": 8, "uint8": 8,
            "bool": 9,
        }
        if has_key and hasattr(type_class, "_key_fields"):
            for field_name, field_type_str in type_class._key_fields:
                # Find field index from dataclass fields
                import dataclasses
                dc_fields = dataclasses.fields(type_class)
                for idx, f in enumerate(dc_fields):
                    if f.name == field_name:
                        key_field_indices.append(idx)
                        key_field_types.append(_KEY_TYPE_MAP.get(field_type_str, 0))
                        break

        # Prefer advertising a conformant TypeObject when the generator emitted flat-type
        # metadata (_dds_type_info_fields) -- this matches the Rust derive so strict XTypes
        # peers can structurally match. Otherwise fall back to the
        # name-based field-descriptor / keyed paths below.
        type_info_fields = getattr(type_class, "_dds_type_info_fields", None)
        # Check for full field descriptors (enables CFT reader-side filtering + compute_key)
        all_fields = getattr(type_class, "_all_fields", None)
        if type_info_fields:
            ti = _build_type_info(self._type_name, extensibility, type_info_fields)
            try:
                check_ret(
                    lib.int2dds_create_topic_with_type_info(
                        participant._handle, topic_name_c, ti, qos_ptr, topic_ptr
                    )
                )
            finally:
                lib.int2dds_type_info_destroy(ti)
        elif all_fields:
            import dataclasses
            dc_fields = dataclasses.fields(type_class)

            name_bufs = []
            name_ptrs = []
            field_type_list = []
            is_key_list = []

            # Build key field names set from _key_fields
            key_names = set()
            if hasattr(type_class, "_key_fields"):
                key_names = {kf[0] for kf in type_class._key_fields}

            for field_name, field_type_str in all_fields:
                buf = ffi.new("char[]", field_name.encode())
                name_bufs.append(buf)
                name_ptrs.append(buf)
                field_type_list.append(_KEY_TYPE_MAP.get(field_type_str, 0))
                is_key_list.append(field_name in key_names)

            names_c = ffi.new("char *[]", name_ptrs)
            types_c = ffi.new("uint32_t[]", field_type_list)
            is_key_c = ffi.new("bool[]", is_key_list)

            check_ret(
                lib.int2dds_create_topic_with_field_descriptors(
                    participant._handle,
                    topic_name_c,
                    type_name_c,
                    int(extensibility),
                    has_key,
                    qos_ptr,
                    names_c,
                    types_c,
                    is_key_c,
                    len(all_fields),
                    topic_ptr,
                )
            )
        elif key_field_indices:
            indices_c = ffi.new("uint32_t[]", key_field_indices)
            types_c = ffi.new("uint32_t[]", key_field_types)
            check_ret(
                lib.int2dds_create_topic_keyed_with_key_fields(
                    participant._handle,
                    topic_name_c,
                    type_name_c,
                    int(extensibility),
                    has_key,
                    qos_ptr,
                    indices_c,
                    types_c,
                    len(key_field_indices),
                    topic_ptr,
                )
            )
        else:
            check_ret(
                lib.int2dds_create_topic_keyed(
                    participant._handle,
                    topic_name_c,
                    type_name_c,
                    int(extensibility),
                    has_key,
                    qos_ptr,
                    topic_ptr,
                )
            )
        self._handle = topic_ptr[0]

        # Clean up QoS handle after use
        if qos_handle is not None:
            lib.int2dds_topic_qos_destroy(qos_handle)

    @classmethod
    def _from_found_handle(
        cls,
        participant: DomainParticipant,
        handle,
        type_class: type[T],
        topic_name: str,
    ) -> "Topic[T]":
        """Wrap a native handle returned by ``find_topic`` without re-creating it.

        The native topic already carries its type registration and (if keyed)
        field descriptors from when it was originally created; this only attaches
        the Python-side ``type_class`` that drives binding serialization.
        """
        obj = object.__new__(cls)
        obj._participant = participant
        obj._name = topic_name
        obj._type_class = type_class
        obj._type_name = getattr(type_class, "_dds_type_name", type_class.__name__)
        obj._handle = handle
        obj._closed = False
        return obj

    @property
    def name(self) -> str:
        """Get the topic name."""
        return self._name

    @property
    def type_name(self) -> str:
        """Get the DDS type name."""
        return self._type_name

    @property
    def type_class(self) -> type[T]:
        """Get the Python type class."""
        return self._type_class

    def get_inconsistent_topic_status(self) -> dict:
        """Get the inconsistent topic status.

        Reports how many times a remote topic with the same name but an
        incompatible type was discovered. Reading the status resets its
        ``total_count_change``.

        Returns:
            dict with total_count, total_count_change
        """
        status = ffi.new("Int2DdsInconsistentTopicStatus *")
        check_ret(lib.int2dds_topic_get_inconsistent_topic_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
        }

    def get_statuscondition(self) -> StatusCondition:
        """Get the StatusCondition associated with this topic."""
        cond_ptr = ffi.new("Int2DdsStatusCondition **")
        check_ret(lib.int2dds_topic_get_statuscondition(self._handle, cond_ptr))
        return StatusCondition(cond_ptr[0], owner=self)

    def get_status_changes(self) -> int:
        """Get the current status change bitmask of this topic."""
        mask_out = ffi.new("uint32_t *")
        check_ret(lib.int2dds_topic_get_status_changes(self._handle, mask_out))
        return mask_out[0]

    def close(self) -> None:
        """Delete the topic."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_delete_topic(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> Topic[T]:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


class ContentFilteredTopic(Generic[T]):
    """
    ContentFilteredTopic - filters data based on a SQL-like expression.

    Created through DomainParticipant.create_contentfilteredtopic().

    Attributes:
        name: The filtered topic name
        type_class: The Python type class for deserialization
        filter_expression: The SQL-92 filter expression
    """

    __slots__ = (
        "_handle", "_participant", "_related_topic", "_name",
        "_type_class", "_filter_expression", "_closed",
    )

    def __init__(
        self,
        participant: DomainParticipant,
        topic_name: str,
        related_topic: Topic[T],
        filter_expression: str,
        expression_parameters: list[str] | None = None,
    ) -> None:
        self._participant = participant
        self._related_topic = related_topic
        self._name = topic_name
        self._type_class = related_topic.type_class
        self._filter_expression = filter_expression
        self._closed = False

        if expression_parameters is None:
            expression_parameters = []

        topic_name_c = ffi.new("char[]", topic_name.encode())
        filter_expr_c = ffi.new("char[]", filter_expression.encode())

        # Build C string array for parameters
        param_ptrs = []
        param_bufs = []
        for p in expression_parameters:
            buf = ffi.new("char[]", p.encode())
            param_bufs.append(buf)
            param_ptrs.append(buf)

        if param_ptrs:
            params_arr = ffi.new("char *[]", param_ptrs)
        else:
            params_arr = ffi.NULL

        cft_ptr = ffi.new("Int2DdsContentFilteredTopic **")
        check_ret(
            lib.int2dds_create_contentfilteredtopic(
                participant._handle,
                topic_name_c,
                related_topic._handle,
                filter_expr_c,
                params_arr,
                len(expression_parameters),
                cft_ptr,
            )
        )
        self._handle = cft_ptr[0]

    @property
    def name(self) -> str:
        return self._name

    @property
    def type_class(self) -> type[T]:
        return self._type_class

    def set_enabled(self, enabled: bool) -> None:
        """Enable or disable filtering at runtime."""
        check_ret(lib.int2dds_contentfilteredtopic_set_enabled(self._handle, enabled))

    def set_expression_parameters(self, parameters: list[str]) -> None:
        """Replace the filter's bound parameters."""
        params_arr, _bufs = _build_cstr_array(parameters)
        check_ret(lib.int2dds_contentfilteredtopic_set_expression_parameters(
            self._handle, params_arr, len(parameters)))

    def set_filter_expression(self, expression: str, parameters: list[str] | None = None) -> None:
        """Replace both the filter expression and its bound parameters."""
        parameters = parameters or []
        expr_c = ffi.new("char[]", expression.encode())
        params_arr, _bufs = _build_cstr_array(parameters)
        check_ret(lib.int2dds_contentfilteredtopic_set_filter_expression(
            self._handle, expr_c, params_arr, len(parameters)))
        self._filter_expression = expression

    def close(self) -> None:
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_delete_contentfilteredtopic(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> ContentFilteredTopic[T]:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass
