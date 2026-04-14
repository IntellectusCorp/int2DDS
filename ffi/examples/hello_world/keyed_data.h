/**
 * CDR serialization helpers for KeyedData type
 *
 * KeyedData layout (Appendable extensibility):
 *   key   : uint32 [KEY]
 *   value : string (bounded 256)
 */
#ifndef KEYED_DATA_IDL_H
#define KEYED_DATA_IDL_H

#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#include "int2dds_cdr.h"

typedef struct KeyedData {
    uint32_t key;
    char value[257];
} KeyedData;

static inline size_t KeyedData_serialize_cdr(
    const KeyedData *val,
    uint8_t *buf,
    size_t capacity)
{
    Int2DdsCdrWriter w;
    int2dds_cdr_writer_init(&w, buf, capacity, true, true);
    int2dds_cdr_write_encapsulation(&w, INT2DDS_CDR_APPENDABLE);
    size_t dh = 0;
    int2dds_cdr_write_dheader_begin(&w, &dh);
    int2dds_cdr_write_u32(&w, val->key);
    int2dds_cdr_write_string(&w, val->value);
    int2dds_cdr_write_dheader_finalize(&w, dh);
    return w.error == INT2DDS_CDR_OK ? int2dds_cdr_writer_size(&w) : 0;
}

static inline bool KeyedData_deserialize_cdr(
    const uint8_t *buf,
    size_t len,
    KeyedData *val_out)
{
    Int2DdsCdrReader r;
    if (int2dds_cdr_reader_init(&r, buf, len) != INT2DDS_CDR_OK)
        return false;
    uint32_t obj_size = 0;
    size_t start_pos = 0;
    int2dds_cdr_read_dheader(&r, &obj_size, &start_pos);
    int2dds_cdr_read_u32(&r, &val_out->key);
    int2dds_cdr_read_string_copy(&r, val_out->value, 257, NULL);
    int2dds_cdr_read_dheader_end(&r, obj_size, start_pos);
    return int2dds_cdr_reader_error(&r) == INT2DDS_CDR_OK;
}

static inline size_t KeyedData_serialize_key(
    const KeyedData *val,
    uint8_t *buf,
    size_t capacity)
{
    Int2DdsCdrWriter w;
    int2dds_cdr_writer_init(&w, buf, capacity, true, true);
    int2dds_cdr_write_encapsulation(&w, INT2DDS_CDR_APPENDABLE);
    size_t dh = 0;
    int2dds_cdr_write_dheader_begin(&w, &dh);
    int2dds_cdr_write_u32(&w, val->key);
    int2dds_cdr_write_dheader_finalize(&w, dh);
    return w.error == INT2DDS_CDR_OK ? int2dds_cdr_writer_size(&w) : 0;
}

#endif /* KEYED_DATA_IDL_H */
