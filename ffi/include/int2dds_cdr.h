/**
 * @file int2dds_cdr.h
 * @brief CDR/XCDR2 Serialization Utility Library for int2DDS
 *
 * This library provides C functions for CDR (Common Data Representation) and
 * XCDR2 (Extended CDR v2) serialization/deserialization. It is designed to be
 * used by IDL-generated code to serialize DDS data types for transmission
 * through the int2DDS FFI layer.
 *
 * ## Usage Modes
 *
 * **Compiled library mode** (default):
 *   Compile `cdr_utils.c` and link against it.
 *
 * **Header-only mode**:
 *   Define `INT2DDS_CDR_STATIC` before including to get `static inline` functions.
 *
 * **Single-file implementation mode**:
 *   Define `INT2DDS_CDR_IMPLEMENTATION` in exactly ONE .c file before including.
 *
 * @copyright IntellectusCorp
 */

#ifndef INT2DDS_CDR_H
#define INT2DDS_CDR_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ========================================================================
 * Linkage Configuration
 * ======================================================================== */

#ifdef INT2DDS_CDR_STATIC
  #define INT2DDS_CDR_DEF static inline
#elif defined(INT2DDS_CDR_IMPLEMENTATION)
  #define INT2DDS_CDR_DEF
#else
  #define INT2DDS_CDR_DEF extern
#endif

/* ========================================================================
 * Error Codes
 * ======================================================================== */

typedef enum Int2DdsCdrError {
    INT2DDS_CDR_OK             = 0,
    INT2DDS_CDR_ERR_OVERFLOW   = 1,       /**< Writer: buffer capacity exceeded   */
    INT2DDS_CDR_ERR_UNDERFLOW  = 2,       /**< Reader: not enough data remaining  */
    INT2DDS_CDR_ERR_INVALID_ENCAP = 5,    /**< Unrecognized encapsulation ID      */
    INT2DDS_CDR_ERR_INVALID_MEMBER_ID = 6,/**< EMHEADER member_id exceeds 28 bits */
    INT2DDS_CDR_ERR_INVALID_ARGUMENT = 7  /**< Bad element size or finalize token */
} Int2DdsCdrError;

/* ========================================================================
 * Encapsulation Encoding IDs
 * ======================================================================== */

#define INT2DDS_CDR_ENCAP_CDR_BE       0x0000
#define INT2DDS_CDR_ENCAP_CDR_LE       0x0001
#define INT2DDS_CDR_ENCAP_PL_CDR_BE    0x0002
#define INT2DDS_CDR_ENCAP_PL_CDR_LE    0x0003
#define INT2DDS_CDR_ENCAP_CDR2_BE      0x0006 /**< PLAINCDR2 BE (Final) */
#define INT2DDS_CDR_ENCAP_CDR2_LE      0x0007 /**< PLAINCDR2 LE (Final) */
#define INT2DDS_CDR_ENCAP_DCDR2_BE     0x0008 /**< DELIMITED_CDR2 BE (Appendable) */
#define INT2DDS_CDR_ENCAP_DCDR2_LE     0x0009 /**< DELIMITED_CDR2 LE (Appendable) */
#define INT2DDS_CDR_ENCAP_PL_CDR2_BE   0x000A /**< PL_CDR2 BE (Mutable) */
#define INT2DDS_CDR_ENCAP_PL_CDR2_LE   0x000B /**< PL_CDR2 LE (Mutable) */

/** Extensibility kinds for int2dds_cdr_write_encapsulation() */
#define INT2DDS_CDR_FINAL       0
#define INT2DDS_CDR_APPENDABLE  1
#define INT2DDS_CDR_MUTABLE     2

/** EMHEADER sentinel member_id (legacy — XCDR2 spec 7.4.3.4 uses DHEADER for end-of-struct; removal scheduled for Tier 2) */
#define INT2DDS_CDR_MEMBER_ID_SENTINEL 0x3F02

/** PL_CDR1 (XCDR1 mutable) reserved parameter IDs */
#define INT2DDS_CDR_PID_EXTENDED     0x3F01
#define INT2DDS_CDR_PID_SENTINEL     0x3F02
#define INT2DDS_CDR_PID_MAX_SHORT_ID 0x3F00

/* ========================================================================
 * Writer Struct
 * ======================================================================== */

typedef struct Int2DdsCdrWriter {
    uint8_t        *buf;
    size_t          capacity;
    size_t          pos;
    size_t          header_size;    /**< 0 or 4 (alignment offset for encap header) */
    bool            little_endian;
    bool            xcdr2;          /**< true: max alignment capped at 4 bytes */
    Int2DdsCdrError error;          /**< sticky error */
} Int2DdsCdrWriter;

/* ========================================================================
 * Reader Struct
 * ======================================================================== */

typedef struct Int2DdsCdrReader {
    const uint8_t  *buf;
    size_t          len;
    size_t          pos;
    size_t          header_size;
    bool            little_endian;
    bool            xcdr2;
    Int2DdsCdrError error;
} Int2DdsCdrReader;

/* ========================================================================
 * Writer Functions
 * ======================================================================== */

INT2DDS_CDR_DEF void    int2dds_cdr_writer_init(Int2DdsCdrWriter *w, uint8_t *buf, size_t capacity, bool little_endian, bool xcdr2);
INT2DDS_CDR_DEF void    int2dds_cdr_writer_reset(Int2DdsCdrWriter *w);
INT2DDS_CDR_DEF size_t  int2dds_cdr_writer_size(const Int2DdsCdrWriter *w);
INT2DDS_CDR_DEF Int2DdsCdrError int2dds_cdr_writer_error(const Int2DdsCdrWriter *w);

INT2DDS_CDR_DEF bool int2dds_cdr_write_encapsulation(Int2DdsCdrWriter *w, int extensibility);
INT2DDS_CDR_DEF bool int2dds_cdr_write_align(Int2DdsCdrWriter *w, size_t alignment);

/* Primitive writes */
INT2DDS_CDR_DEF bool int2dds_cdr_write_bool(Int2DdsCdrWriter *w, bool val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_i8  (Int2DdsCdrWriter *w, int8_t val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_u8  (Int2DdsCdrWriter *w, uint8_t val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_i16 (Int2DdsCdrWriter *w, int16_t val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_u16 (Int2DdsCdrWriter *w, uint16_t val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_i32 (Int2DdsCdrWriter *w, int32_t val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_u32 (Int2DdsCdrWriter *w, uint32_t val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_i64 (Int2DdsCdrWriter *w, int64_t val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_u64 (Int2DdsCdrWriter *w, uint64_t val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_f32 (Int2DdsCdrWriter *w, float val);
INT2DDS_CDR_DEF bool int2dds_cdr_write_f64 (Int2DdsCdrWriter *w, double val);

/* String / Sequence / Bytes */
INT2DDS_CDR_DEF bool int2dds_cdr_write_string(Int2DdsCdrWriter *w, const char *str);
INT2DDS_CDR_DEF bool int2dds_cdr_write_seq_header(Int2DdsCdrWriter *w, uint32_t count);
INT2DDS_CDR_DEF bool int2dds_cdr_write_bytes(Int2DdsCdrWriter *w, const uint8_t *data, size_t len);

/* Bulk primitive array/sequence body. `elem_size` must be 1, 2, 4 or 8 and must equal
 * sizeof(*data). Aligns once, then copies the whole run — byte-identical to writing the
 * elements one at a time, because CDR lays out same-width primitives with no inter-element
 * padding. A zero count writes nothing at all (not even alignment padding), matching a
 * per-element loop that never executes. */
INT2DDS_CDR_DEF bool int2dds_cdr_write_prim_array(Int2DdsCdrWriter *w, const void *data, size_t count, size_t elem_size);

/* XCDR2 DHEADER */
INT2DDS_CDR_DEF bool int2dds_cdr_write_dheader_begin(Int2DdsCdrWriter *w, size_t *token_out);
INT2DDS_CDR_DEF bool int2dds_cdr_write_dheader_finalize(Int2DdsCdrWriter *w, size_t token);

/* XCDR2 EMHEADER */
INT2DDS_CDR_DEF bool int2dds_cdr_write_emheader(Int2DdsCdrWriter *w, uint32_t member_id, uint32_t data_length, bool must_understand);
INT2DDS_CDR_DEF bool int2dds_cdr_write_emheader_begin(Int2DdsCdrWriter *w, uint32_t member_id, bool must_understand, size_t *token_out);
INT2DDS_CDR_DEF bool int2dds_cdr_write_emheader_finalize(Int2DdsCdrWriter *w, size_t token);
INT2DDS_CDR_DEF bool int2dds_cdr_write_sentinel(Int2DdsCdrWriter *w);

/* PL_CDR1 (XCDR1 mutable) parameter headers */
INT2DDS_CDR_DEF bool int2dds_cdr_write_pid_begin(Int2DdsCdrWriter *w, uint32_t member_id, size_t *token_out);
INT2DDS_CDR_DEF bool int2dds_cdr_write_pid_finalize(Int2DdsCdrWriter *w, uint32_t member_id, bool must_understand, size_t token);
INT2DDS_CDR_DEF bool int2dds_cdr_write_pid_sentinel(Int2DdsCdrWriter *w);

/* Enum (i32 wrapper) */
INT2DDS_CDR_DEF bool int2dds_cdr_write_enum(Int2DdsCdrWriter *w, int32_t discriminant);

/* ========================================================================
 * Reader Functions
 * ======================================================================== */

INT2DDS_CDR_DEF Int2DdsCdrError int2dds_cdr_reader_init(Int2DdsCdrReader *r, const uint8_t *buf, size_t len);
INT2DDS_CDR_DEF void    int2dds_cdr_reader_init_no_header(Int2DdsCdrReader *r, const uint8_t *buf, size_t len, bool little_endian, bool xcdr2);
INT2DDS_CDR_DEF size_t  int2dds_cdr_reader_remaining(const Int2DdsCdrReader *r);
INT2DDS_CDR_DEF size_t  int2dds_cdr_reader_position(const Int2DdsCdrReader *r);
INT2DDS_CDR_DEF Int2DdsCdrError int2dds_cdr_reader_error(const Int2DdsCdrReader *r);
INT2DDS_CDR_DEF bool    int2dds_cdr_read_align(Int2DdsCdrReader *r, size_t alignment);

/* Primitive reads */
INT2DDS_CDR_DEF bool int2dds_cdr_read_bool(Int2DdsCdrReader *r, bool *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_i8  (Int2DdsCdrReader *r, int8_t *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_u8  (Int2DdsCdrReader *r, uint8_t *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_i16 (Int2DdsCdrReader *r, int16_t *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_u16 (Int2DdsCdrReader *r, uint16_t *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_i32 (Int2DdsCdrReader *r, int32_t *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_u32 (Int2DdsCdrReader *r, uint32_t *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_i64 (Int2DdsCdrReader *r, int64_t *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_u64 (Int2DdsCdrReader *r, uint64_t *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_f32 (Int2DdsCdrReader *r, float *out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_f64 (Int2DdsCdrReader *r, double *out);

/* String / Sequence / Bytes */
INT2DDS_CDR_DEF bool int2dds_cdr_read_string(Int2DdsCdrReader *r, const char **str_out, size_t *len_out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_string_copy(Int2DdsCdrReader *r, char *buf, size_t buf_capacity, size_t *actual_len_out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_seq_header(Int2DdsCdrReader *r, uint32_t *count_out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_bytes(Int2DdsCdrReader *r, uint8_t *out, size_t len);

/* Bulk counterpart of int2dds_cdr_write_prim_array. */
INT2DDS_CDR_DEF bool int2dds_cdr_read_prim_array(Int2DdsCdrReader *r, void *out, size_t count, size_t elem_size);

/* Bulk boolean read. Not a plain copy: the wire may carry any octet, and storing anything
 * other than 0 or 1 in a C `bool` is undefined, so each octet is normalized the way
 * int2dds_cdr_read_bool() does. Booleans are written with int2dds_cdr_write_prim_array()
 * instead — a valid `bool` already holds 0 or 1, so there is nothing to normalize. */
INT2DDS_CDR_DEF bool int2dds_cdr_read_bool_array(Int2DdsCdrReader *r, bool *out, size_t count);

/* XCDR2 DHEADER */
INT2DDS_CDR_DEF bool int2dds_cdr_read_dheader(Int2DdsCdrReader *r, uint32_t *object_size_out, size_t *start_pos_out);
INT2DDS_CDR_DEF bool int2dds_cdr_read_dheader_end(Int2DdsCdrReader *r, uint32_t object_size, size_t start_pos);

/* XCDR2 EMHEADER */
INT2DDS_CDR_DEF bool int2dds_cdr_read_emheader(Int2DdsCdrReader *r, uint32_t *member_id_out, uint32_t *data_length_out, bool *must_understand_out);
INT2DDS_CDR_DEF bool int2dds_cdr_is_sentinel(const Int2DdsCdrReader *r);
INT2DDS_CDR_DEF bool int2dds_cdr_skip_bytes(Int2DdsCdrReader *r, size_t nbytes);

/* PL_CDR1 (XCDR1 mutable) parameter headers */
INT2DDS_CDR_DEF bool int2dds_cdr_read_pid_header(Int2DdsCdrReader *r, uint32_t *member_id_out, uint32_t *data_length_out, bool *sentinel_out);

/* Enum (i32 wrapper) */
INT2DDS_CDR_DEF bool int2dds_cdr_read_enum(Int2DdsCdrReader *r, int32_t *discriminant_out);

/* ========================================================================
 * Implementation
 * ======================================================================== */

#if defined(INT2DDS_CDR_IMPLEMENTATION) || defined(INT2DDS_CDR_STATIC)

/* ---- Internal helpers ------------------------------------------------- */

static inline bool _int2dds_cdr_w_ok(Int2DdsCdrWriter *w) {
    return w->error == INT2DDS_CDR_OK;
}

static inline bool _int2dds_cdr_r_ok(Int2DdsCdrReader *r) {
    return r->error == INT2DDS_CDR_OK;
}

/* Subtraction form: `pos + n` can wrap and pass an additive check, which is
 * reachable from a wire uint32 length wherever size_t is 32 bits. */
static inline bool _int2dds_cdr_w_ensure(Int2DdsCdrWriter *w, size_t n) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (w->pos > w->capacity || n > w->capacity - w->pos) {
        w->error = INT2DDS_CDR_ERR_OVERFLOW;
        return false;
    }
    return true;
}

static inline bool _int2dds_cdr_r_ensure(Int2DdsCdrReader *r, size_t n) {
    if (!_int2dds_cdr_r_ok(r)) return false;
    if (r->pos > r->len || n > r->len - r->pos) {
        r->error = INT2DDS_CDR_ERR_UNDERFLOW;
        return false;
    }
    return true;
}

/* Bulk APIs divide by elem_size to bound `count`, so 0 has to be rejected up
 * front. Only these four widths are supported. */
static inline bool _int2dds_cdr_elem_size_ok(size_t elem_size) {
    return elem_size == 1 || elem_size == 2 || elem_size == 4 || elem_size == 8;
}

/* A finalize() patches header_bytes in place at `token` and derives a length from
 * `pos - token`. Both are unsafe unless the token names a header this writer
 * actually reserved, so the patch region stays inside [0, pos) and thus [0, capacity). */
static inline bool _int2dds_cdr_token_ok(const Int2DdsCdrWriter *w, size_t token, size_t header_bytes) {
    if (w->pos > w->capacity) return false;
    if (token > w->pos) return false;
    return w->pos - token >= header_bytes;
}

/* ---- Endian byte-swap helpers (strict-aliasing safe via memcpy) -------- */

static inline void _int2dds_put_u16(uint8_t *dst, uint16_t val, bool le) {
    if (le) {
        dst[0] = (uint8_t)(val);
        dst[1] = (uint8_t)(val >> 8);
    } else {
        dst[0] = (uint8_t)(val >> 8);
        dst[1] = (uint8_t)(val);
    }
}

static inline uint16_t _int2dds_get_u16(const uint8_t *src, bool le) {
    if (le) return (uint16_t)src[0] | ((uint16_t)src[1] << 8);
    else    return ((uint16_t)src[0] << 8) | (uint16_t)src[1];
}

static inline void _int2dds_put_u32(uint8_t *dst, uint32_t val, bool le) {
    if (le) {
        dst[0] = (uint8_t)(val);
        dst[1] = (uint8_t)(val >> 8);
        dst[2] = (uint8_t)(val >> 16);
        dst[3] = (uint8_t)(val >> 24);
    } else {
        dst[0] = (uint8_t)(val >> 24);
        dst[1] = (uint8_t)(val >> 16);
        dst[2] = (uint8_t)(val >> 8);
        dst[3] = (uint8_t)(val);
    }
}

static inline uint32_t _int2dds_get_u32(const uint8_t *src, bool le) {
    if (le) return (uint32_t)src[0] | ((uint32_t)src[1] << 8)
                | ((uint32_t)src[2] << 16) | ((uint32_t)src[3] << 24);
    else    return ((uint32_t)src[0] << 24) | ((uint32_t)src[1] << 16)
                | ((uint32_t)src[2] << 8) | (uint32_t)src[3];
}

static inline void _int2dds_put_u64(uint8_t *dst, uint64_t val, bool le) {
    if (le) {
        dst[0] = (uint8_t)(val);       dst[1] = (uint8_t)(val >> 8);
        dst[2] = (uint8_t)(val >> 16);  dst[3] = (uint8_t)(val >> 24);
        dst[4] = (uint8_t)(val >> 32);  dst[5] = (uint8_t)(val >> 40);
        dst[6] = (uint8_t)(val >> 48);  dst[7] = (uint8_t)(val >> 56);
    } else {
        dst[0] = (uint8_t)(val >> 56);  dst[1] = (uint8_t)(val >> 48);
        dst[2] = (uint8_t)(val >> 40);  dst[3] = (uint8_t)(val >> 32);
        dst[4] = (uint8_t)(val >> 24);  dst[5] = (uint8_t)(val >> 16);
        dst[6] = (uint8_t)(val >> 8);   dst[7] = (uint8_t)(val);
    }
}

static inline uint64_t _int2dds_get_u64(const uint8_t *src, bool le) {
    if (le) return (uint64_t)src[0] | ((uint64_t)src[1] << 8)
                | ((uint64_t)src[2] << 16) | ((uint64_t)src[3] << 24)
                | ((uint64_t)src[4] << 32) | ((uint64_t)src[5] << 40)
                | ((uint64_t)src[6] << 48) | ((uint64_t)src[7] << 56);
    else    return ((uint64_t)src[0] << 56) | ((uint64_t)src[1] << 48)
                | ((uint64_t)src[2] << 40) | ((uint64_t)src[3] << 32)
                | ((uint64_t)src[4] << 24) | ((uint64_t)src[5] << 16)
                | ((uint64_t)src[6] << 8) | (uint64_t)src[7];
}

/* ---- Bulk primitive copy helpers -------------------------------------- */

/* Host byte order, resolved at compile time where the toolchain exposes it and by a
 * folded runtime probe otherwise. Used to decide whether a primitive run can go out
 * as a straight memcpy or has to be swapped element by element. */
#if defined(__BYTE_ORDER__) && defined(__ORDER_BIG_ENDIAN__)
  #define INT2DDS_CDR_HOST_LE (__BYTE_ORDER__ != __ORDER_BIG_ENDIAN__)
#elif defined(_MSC_VER)
  #define INT2DDS_CDR_HOST_LE 1  /* every MSVC target is little-endian */
#else
static inline bool _int2dds_cdr_host_le_probe(void) {
    const uint16_t one = 1;
    return *(const uint8_t *)&one != 0;
}
  #define INT2DDS_CDR_HOST_LE _int2dds_cdr_host_le_probe()
#endif

/* CDR encodes boolean as a single octet, and the bulk bool paths stride by 1.
 * Every ABI int2DDS targets has sizeof(bool) == 1; fail the build rather than
 * mis-stride if that ever stops holding. */
typedef char _int2dds_cdr_bool_is_one_byte[(sizeof(bool) == 1) ? 1 : -1];

/* Reverse each element while copying. Only reached when the stream byte order differs
 * from the host, which cannot happen on the common LE-host/LE-wire path. */
static inline void _int2dds_cdr_swap_copy(uint8_t *dst, const uint8_t *src,
                                          size_t count, size_t elem_size) {
    for (size_t i = 0; i < count; i++) {
        const uint8_t *s = src + i * elem_size;
        uint8_t       *d = dst + (i + 1) * elem_size;
        for (size_t b = 0; b < elem_size; b++) *(--d) = *s++;
    }
}

/* ---- Writer Init ------------------------------------------------------ */

INT2DDS_CDR_DEF void int2dds_cdr_writer_init(Int2DdsCdrWriter *w, uint8_t *buf, size_t capacity, bool little_endian, bool xcdr2) {
    w->buf           = buf;
    w->capacity      = capacity;
    w->pos           = 0;
    w->header_size   = 0;
    w->little_endian = little_endian;
    w->xcdr2         = xcdr2;
    w->error         = INT2DDS_CDR_OK;
}

INT2DDS_CDR_DEF void int2dds_cdr_writer_reset(Int2DdsCdrWriter *w) {
    w->pos         = 0;
    w->header_size = 0;
    w->error       = INT2DDS_CDR_OK;
}

INT2DDS_CDR_DEF size_t int2dds_cdr_writer_size(const Int2DdsCdrWriter *w) {
    return w->pos;
}

INT2DDS_CDR_DEF Int2DdsCdrError int2dds_cdr_writer_error(const Int2DdsCdrWriter *w) {
    return w->error;
}

/* ---- Writer Alignment ------------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_write_align(Int2DdsCdrWriter *w, size_t alignment) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (alignment <= 1) return true;

    /* XCDR2: max alignment capped at 4 */
    size_t actual = w->xcdr2 ? (alignment < 4 ? alignment : 4) : alignment;

    /* Alignment relative to data start (after encapsulation header) */
    size_t stream_pos = w->pos - w->header_size;
    size_t aligned    = (stream_pos + actual - 1) & ~(actual - 1);
    size_t padding    = aligned - stream_pos;

    if (padding == 0) return true;
    if (!_int2dds_cdr_w_ensure(w, padding)) return false;

    memset(w->buf + w->pos, 0, padding);
    w->pos += padding;
    return true;
}

/* ---- Encapsulation Header --------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_write_encapsulation(Int2DdsCdrWriter *w, int extensibility) {
    if (!_int2dds_cdr_w_ensure(w, 4)) return false;

    uint16_t encap_id;
    if (w->xcdr2) {
        switch (extensibility) {
        case INT2DDS_CDR_FINAL:
            encap_id = w->little_endian ? INT2DDS_CDR_ENCAP_CDR2_LE : INT2DDS_CDR_ENCAP_CDR2_BE;
            break;
        case INT2DDS_CDR_APPENDABLE:
            encap_id = w->little_endian ? INT2DDS_CDR_ENCAP_DCDR2_LE : INT2DDS_CDR_ENCAP_DCDR2_BE;
            break;
        case INT2DDS_CDR_MUTABLE:
            encap_id = w->little_endian ? INT2DDS_CDR_ENCAP_PL_CDR2_LE : INT2DDS_CDR_ENCAP_PL_CDR2_BE;
            break;
        default:
            encap_id = w->little_endian ? INT2DDS_CDR_ENCAP_CDR2_LE : INT2DDS_CDR_ENCAP_CDR2_BE;
            break;
        }
    } else if (extensibility == INT2DDS_CDR_MUTABLE) {
        encap_id = w->little_endian ? INT2DDS_CDR_ENCAP_PL_CDR_LE : INT2DDS_CDR_ENCAP_PL_CDR_BE;
    } else {
        encap_id = w->little_endian ? INT2DDS_CDR_ENCAP_CDR_LE : INT2DDS_CDR_ENCAP_CDR_BE;
    }

    /* Encapsulation header is always big-endian */
    w->buf[w->pos + 0] = (uint8_t)(encap_id >> 8);
    w->buf[w->pos + 1] = (uint8_t)(encap_id);
    w->buf[w->pos + 2] = 0; /* options */
    w->buf[w->pos + 3] = 0;
    w->pos += 4;
    w->header_size = 4;
    return true;
}

/* ---- Primitive Writes ------------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_write_bool(Int2DdsCdrWriter *w, bool val) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (!_int2dds_cdr_w_ensure(w, 1)) return false;
    w->buf[w->pos++] = val ? 1 : 0;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_i8(Int2DdsCdrWriter *w, int8_t val) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (!_int2dds_cdr_w_ensure(w, 1)) return false;
    w->buf[w->pos++] = (uint8_t)val;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_u8(Int2DdsCdrWriter *w, uint8_t val) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (!_int2dds_cdr_w_ensure(w, 1)) return false;
    w->buf[w->pos++] = val;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_i16(Int2DdsCdrWriter *w, int16_t val) {
    if (!int2dds_cdr_write_align(w, 2)) return false;
    if (!_int2dds_cdr_w_ensure(w, 2)) return false;
    _int2dds_put_u16(w->buf + w->pos, (uint16_t)val, w->little_endian);
    w->pos += 2;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_u16(Int2DdsCdrWriter *w, uint16_t val) {
    if (!int2dds_cdr_write_align(w, 2)) return false;
    if (!_int2dds_cdr_w_ensure(w, 2)) return false;
    _int2dds_put_u16(w->buf + w->pos, val, w->little_endian);
    w->pos += 2;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_i32(Int2DdsCdrWriter *w, int32_t val) {
    if (!int2dds_cdr_write_align(w, 4)) return false;
    if (!_int2dds_cdr_w_ensure(w, 4)) return false;
    _int2dds_put_u32(w->buf + w->pos, (uint32_t)val, w->little_endian);
    w->pos += 4;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_u32(Int2DdsCdrWriter *w, uint32_t val) {
    if (!int2dds_cdr_write_align(w, 4)) return false;
    if (!_int2dds_cdr_w_ensure(w, 4)) return false;
    _int2dds_put_u32(w->buf + w->pos, val, w->little_endian);
    w->pos += 4;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_i64(Int2DdsCdrWriter *w, int64_t val) {
    if (!int2dds_cdr_write_align(w, 8)) return false;
    if (!_int2dds_cdr_w_ensure(w, 8)) return false;
    _int2dds_put_u64(w->buf + w->pos, (uint64_t)val, w->little_endian);
    w->pos += 8;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_u64(Int2DdsCdrWriter *w, uint64_t val) {
    if (!int2dds_cdr_write_align(w, 8)) return false;
    if (!_int2dds_cdr_w_ensure(w, 8)) return false;
    _int2dds_put_u64(w->buf + w->pos, val, w->little_endian);
    w->pos += 8;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_f32(Int2DdsCdrWriter *w, float val) {
    uint32_t bits;
    memcpy(&bits, &val, 4);
    return int2dds_cdr_write_u32(w, bits);
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_f64(Int2DdsCdrWriter *w, double val) {
    uint64_t bits;
    memcpy(&bits, &val, 8);
    return int2dds_cdr_write_u64(w, bits);
}

/* ---- String / Sequence / Bytes Write ---------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_write_string(Int2DdsCdrWriter *w, const char *str) {
    size_t slen = str ? strlen(str) : 0;
    uint32_t cdr_len = (uint32_t)(slen + 1); /* includes null terminator */
    if (!int2dds_cdr_write_u32(w, cdr_len)) return false;
    if (!_int2dds_cdr_w_ensure(w, slen + 1)) return false;
    if (slen > 0) memcpy(w->buf + w->pos, str, slen);
    w->buf[w->pos + slen] = 0;
    w->pos += slen + 1;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_seq_header(Int2DdsCdrWriter *w, uint32_t count) {
    return int2dds_cdr_write_u32(w, count);
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_bytes(Int2DdsCdrWriter *w, const uint8_t *data, size_t len) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (!_int2dds_cdr_w_ensure(w, len)) return false;
    if (len > 0) memcpy(w->buf + w->pos, data, len);
    w->pos += len;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_prim_array(Int2DdsCdrWriter *w, const void *data,
                                                  size_t count, size_t elem_size) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (!_int2dds_cdr_elem_size_ok(elem_size)) {
        w->error = INT2DDS_CDR_ERR_INVALID_ARGUMENT;
        return false;
    }
    /* No element means nothing to align to: a per-element loop would emit no padding. */
    if (count == 0) return true;
    if (count > (size_t)-1 / elem_size) {
        w->error = INT2DDS_CDR_ERR_OVERFLOW;
        return false;
    }
    if (elem_size > 1 && !int2dds_cdr_write_align(w, elem_size)) return false;

    size_t n = count * elem_size;
    if (!_int2dds_cdr_w_ensure(w, n)) return false;
    if (elem_size == 1 || w->little_endian == INT2DDS_CDR_HOST_LE) {
        memcpy(w->buf + w->pos, data, n);
    } else {
        _int2dds_cdr_swap_copy(w->buf + w->pos, (const uint8_t *)data, count, elem_size);
    }
    w->pos += n;
    return true;
}


/* ---- XCDR2 DHEADER Write ---------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_write_dheader_begin(Int2DdsCdrWriter *w, size_t *token_out) {
    if (!int2dds_cdr_write_align(w, 4)) return false;
    *token_out = w->pos;
    /* Write placeholder (0) */
    return int2dds_cdr_write_u32(w, 0);
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_dheader_finalize(Int2DdsCdrWriter *w, size_t token) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (!_int2dds_cdr_token_ok(w, token, 4)) {
        w->error = INT2DDS_CDR_ERR_INVALID_ARGUMENT;
        return false;
    }
    uint32_t object_size = (uint32_t)(w->pos - token - 4);
    _int2dds_put_u32(w->buf + token, object_size, w->little_endian);
    return true;
}

/* ---- XCDR2 EMHEADER Write --------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_write_emheader(Int2DdsCdrWriter *w, uint32_t member_id, uint32_t data_length, bool must_understand) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (member_id > 0x0FFFFFFFu) {
        w->error = INT2DDS_CDR_ERR_INVALID_MEMBER_ID;
        return false;
    }
    uint32_t mu_bit = must_understand ? 0x80000000u : 0;
    uint32_t lc_word;
    bool needs_nextint;
    switch (data_length) {
        case 1: lc_word = 0u << 28; needs_nextint = false; break;
        case 2: lc_word = 1u << 28; needs_nextint = false; break;
        case 4: lc_word = 2u << 28; needs_nextint = false; break;
        case 8: lc_word = 3u << 28; needs_nextint = false; break;
        default: lc_word = 4u << 28; needs_nextint = true; break;
    }
    uint32_t header = mu_bit | lc_word | (member_id & 0x0FFFFFFFu);
    if (!int2dds_cdr_write_u32(w, header)) return false;
    if (needs_nextint) {
        return int2dds_cdr_write_u32(w, data_length);
    }
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_emheader_begin(Int2DdsCdrWriter *w, uint32_t member_id, bool must_understand, size_t *token_out) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (member_id > 0x0FFFFFFFu) {
        w->error = INT2DDS_CDR_ERR_INVALID_MEMBER_ID;
        return false;
    }
    uint32_t mu_bit = must_understand ? 0x80000000u : 0;
    uint32_t header = mu_bit | (4u << 28) | (member_id & 0x0FFFFFFFu);
    if (!int2dds_cdr_write_u32(w, header)) return false;
    *token_out = w->pos;
    return int2dds_cdr_write_u32(w, 0);
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_emheader_finalize(Int2DdsCdrWriter *w, size_t token) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    if (!_int2dds_cdr_token_ok(w, token, 4)) {
        w->error = INT2DDS_CDR_ERR_INVALID_ARGUMENT;
        return false;
    }
    uint32_t data_length = (uint32_t)(w->pos - token - 4);
    _int2dds_put_u32(w->buf + token, data_length, w->little_endian);
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_sentinel(Int2DdsCdrWriter *w) {
    uint32_t header = (uint32_t)INT2DDS_CDR_MEMBER_ID_SENTINEL & 0x0FFFFFFFu;
    return int2dds_cdr_write_u32(w, header);
}

/* ---- PL_CDR1 (XCDR1 mutable) Parameter Headers ------------------------ */

INT2DDS_CDR_DEF bool int2dds_cdr_write_pid_begin(Int2DdsCdrWriter *w, uint32_t member_id, size_t *token_out) {
    if (!int2dds_cdr_write_align(w, 4)) return false;
    size_t reserve = (member_id <= INT2DDS_CDR_PID_MAX_SHORT_ID) ? 4 : 12;
    if (!_int2dds_cdr_w_ensure(w, reserve)) return false;
    *token_out = w->pos;
    memset(w->buf + w->pos, 0, reserve);
    w->pos += reserve;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_pid_finalize(Int2DdsCdrWriter *w, uint32_t member_id, bool must_understand, size_t token) {
    if (!_int2dds_cdr_w_ok(w)) return false;
    uint16_t flags = must_understand ? 0x4000 : 0;
    bool short_hdr = member_id <= INT2DDS_CDR_PID_MAX_SHORT_ID;
    size_t header_bytes = short_hdr ? 4u : 12u;
    if (!_int2dds_cdr_token_ok(w, token, header_bytes)) {
        w->error = INT2DDS_CDR_ERR_INVALID_ARGUMENT;
        return false;
    }
    size_t content_len = w->pos - token - header_bytes;

    if (short_hdr && content_len <= 0xFFFF) {
        _int2dds_put_u16(w->buf + token, (uint16_t)(flags | (member_id & 0x3FFFu)), w->little_endian);
        _int2dds_put_u16(w->buf + token + 2, (uint16_t)content_len, w->little_endian);
        return true;
    }
    if (short_hdr) {
        /* Length outgrew the short header: widen to the extended form in place */
        if (!_int2dds_cdr_w_ensure(w, 8)) return false;
        memmove(w->buf + token + 12, w->buf + token + 4, content_len);
        w->pos += 8;
    }
    _int2dds_put_u16(w->buf + token, (uint16_t)(flags | INT2DDS_CDR_PID_EXTENDED), w->little_endian);
    _int2dds_put_u16(w->buf + token + 2, 8, w->little_endian);
    _int2dds_put_u32(w->buf + token + 4, member_id, w->little_endian);
    _int2dds_put_u32(w->buf + token + 8, (uint32_t)content_len, w->little_endian);
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_write_pid_sentinel(Int2DdsCdrWriter *w) {
    if (!int2dds_cdr_write_align(w, 4)) return false;
    if (!_int2dds_cdr_w_ensure(w, 4)) return false;
    _int2dds_put_u16(w->buf + w->pos, INT2DDS_CDR_PID_SENTINEL, w->little_endian);
    _int2dds_put_u16(w->buf + w->pos + 2, 0, w->little_endian);
    w->pos += 4;
    return true;
}

/* ---- Enum Write ------------------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_write_enum(Int2DdsCdrWriter *w, int32_t discriminant) {
    return int2dds_cdr_write_i32(w, discriminant);
}

/* ---- Reader Init ------------------------------------------------------ */

INT2DDS_CDR_DEF Int2DdsCdrError int2dds_cdr_reader_init(Int2DdsCdrReader *r, const uint8_t *buf, size_t len) {
    if (len < 4) return INT2DDS_CDR_ERR_UNDERFLOW;

    /* Parse encapsulation header (always big-endian) */
    uint16_t encap_id = ((uint16_t)buf[0] << 8) | (uint16_t)buf[1];

    r->buf   = buf;
    r->len   = len;
    r->error = INT2DDS_CDR_OK;

    switch (encap_id) {
    case INT2DDS_CDR_ENCAP_CDR_LE:
        r->little_endian = true;  r->xcdr2 = false; break;
    case INT2DDS_CDR_ENCAP_CDR_BE:
        r->little_endian = false; r->xcdr2 = false; break;
    case INT2DDS_CDR_ENCAP_CDR2_LE:
    case INT2DDS_CDR_ENCAP_DCDR2_LE:
    case INT2DDS_CDR_ENCAP_PL_CDR2_LE:
        r->little_endian = true;  r->xcdr2 = true;  break;
    case INT2DDS_CDR_ENCAP_CDR2_BE:
    case INT2DDS_CDR_ENCAP_DCDR2_BE:
    case INT2DDS_CDR_ENCAP_PL_CDR2_BE:
        r->little_endian = false; r->xcdr2 = true;  break;
    case INT2DDS_CDR_ENCAP_PL_CDR_LE:
        r->little_endian = true;  r->xcdr2 = false; break;
    case INT2DDS_CDR_ENCAP_PL_CDR_BE:
        r->little_endian = false; r->xcdr2 = false; break;
    default:
        return INT2DDS_CDR_ERR_INVALID_ENCAP;
    }

    r->header_size = 4;
    r->pos = 4; /* skip encapsulation header */
    return INT2DDS_CDR_OK;
}

INT2DDS_CDR_DEF void int2dds_cdr_reader_init_no_header(Int2DdsCdrReader *r, const uint8_t *buf, size_t len, bool little_endian, bool xcdr2) {
    r->buf           = buf;
    r->len           = len;
    r->pos           = 0;
    r->header_size   = 0;
    r->little_endian = little_endian;
    r->xcdr2         = xcdr2;
    r->error         = INT2DDS_CDR_OK;
}

INT2DDS_CDR_DEF size_t int2dds_cdr_reader_remaining(const Int2DdsCdrReader *r) {
    return (r->pos < r->len) ? (r->len - r->pos) : 0;
}

INT2DDS_CDR_DEF size_t int2dds_cdr_reader_position(const Int2DdsCdrReader *r) {
    return r->pos;
}

INT2DDS_CDR_DEF Int2DdsCdrError int2dds_cdr_reader_error(const Int2DdsCdrReader *r) {
    return r->error;
}

/* ---- Reader Alignment ------------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_read_align(Int2DdsCdrReader *r, size_t alignment) {
    if (!_int2dds_cdr_r_ok(r)) return false;
    if (alignment <= 1) return true;

    size_t actual = r->xcdr2 ? (alignment < 4 ? alignment : 4) : alignment;

    size_t stream_pos = r->pos - r->header_size;
    size_t aligned    = (stream_pos + actual - 1) & ~(actual - 1);
    size_t new_pos    = aligned + r->header_size;

    if (new_pos > r->len) {
        r->error = INT2DDS_CDR_ERR_UNDERFLOW;
        return false;
    }
    r->pos = new_pos;
    return true;
}

/* ---- Primitive Reads -------------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_read_bool(Int2DdsCdrReader *r, bool *out) {
    if (!_int2dds_cdr_r_ensure(r, 1)) { *out = false; return false; }
    *out = r->buf[r->pos++] != 0;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_i8(Int2DdsCdrReader *r, int8_t *out) {
    if (!_int2dds_cdr_r_ensure(r, 1)) { *out = 0; return false; }
    *out = (int8_t)r->buf[r->pos++];
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_u8(Int2DdsCdrReader *r, uint8_t *out) {
    if (!_int2dds_cdr_r_ensure(r, 1)) { *out = 0; return false; }
    *out = r->buf[r->pos++];
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_i16(Int2DdsCdrReader *r, int16_t *out) {
    if (!int2dds_cdr_read_align(r, 2)) { *out = 0; return false; }
    if (!_int2dds_cdr_r_ensure(r, 2)) { *out = 0; return false; }
    *out = (int16_t)_int2dds_get_u16(r->buf + r->pos, r->little_endian);
    r->pos += 2;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_u16(Int2DdsCdrReader *r, uint16_t *out) {
    if (!int2dds_cdr_read_align(r, 2)) { *out = 0; return false; }
    if (!_int2dds_cdr_r_ensure(r, 2)) { *out = 0; return false; }
    *out = _int2dds_get_u16(r->buf + r->pos, r->little_endian);
    r->pos += 2;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_i32(Int2DdsCdrReader *r, int32_t *out) {
    if (!int2dds_cdr_read_align(r, 4)) { *out = 0; return false; }
    if (!_int2dds_cdr_r_ensure(r, 4)) { *out = 0; return false; }
    *out = (int32_t)_int2dds_get_u32(r->buf + r->pos, r->little_endian);
    r->pos += 4;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_u32(Int2DdsCdrReader *r, uint32_t *out) {
    if (!int2dds_cdr_read_align(r, 4)) { *out = 0; return false; }
    if (!_int2dds_cdr_r_ensure(r, 4)) { *out = 0; return false; }
    *out = _int2dds_get_u32(r->buf + r->pos, r->little_endian);
    r->pos += 4;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_i64(Int2DdsCdrReader *r, int64_t *out) {
    if (!int2dds_cdr_read_align(r, 8)) { *out = 0; return false; }
    if (!_int2dds_cdr_r_ensure(r, 8)) { *out = 0; return false; }
    *out = (int64_t)_int2dds_get_u64(r->buf + r->pos, r->little_endian);
    r->pos += 8;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_u64(Int2DdsCdrReader *r, uint64_t *out) {
    if (!int2dds_cdr_read_align(r, 8)) { *out = 0; return false; }
    if (!_int2dds_cdr_r_ensure(r, 8)) { *out = 0; return false; }
    *out = _int2dds_get_u64(r->buf + r->pos, r->little_endian);
    r->pos += 8;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_f32(Int2DdsCdrReader *r, float *out) {
    uint32_t bits;
    if (!int2dds_cdr_read_u32(r, &bits)) { *out = 0.0f; return false; }
    memcpy(out, &bits, 4);
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_f64(Int2DdsCdrReader *r, double *out) {
    uint64_t bits;
    if (!int2dds_cdr_read_u64(r, &bits)) { *out = 0.0; return false; }
    memcpy(out, &bits, 8);
    return true;
}

/* ---- String / Sequence / Bytes Read ----------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_read_string(Int2DdsCdrReader *r, const char **str_out, size_t *len_out) {
    uint32_t cdr_len;
    if (!int2dds_cdr_read_u32(r, &cdr_len)) {
        *str_out = NULL; *len_out = 0;
        return false;
    }
    if (cdr_len == 0) {
        *str_out = (const char *)(r->buf + r->pos); /* points to valid memory */
        *len_out = 0;
        return true;
    }
    if (!_int2dds_cdr_r_ensure(r, cdr_len)) {
        *str_out = NULL; *len_out = 0;
        return false;
    }
    *str_out = (const char *)(r->buf + r->pos);
    *len_out = cdr_len - 1; /* exclude null terminator */
    r->pos += cdr_len;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_string_copy(Int2DdsCdrReader *r, char *buf, size_t buf_capacity, size_t *actual_len_out) {
    const char *str;
    size_t slen;
    if (!int2dds_cdr_read_string(r, &str, &slen)) {
        if (actual_len_out) *actual_len_out = 0;
        return false;
    }
    size_t needed = slen + 1;
    if (actual_len_out) *actual_len_out = needed;
    if (needed > buf_capacity) {
        r->error = INT2DDS_CDR_ERR_OVERFLOW;
        return false;
    }
    if (slen > 0) memcpy(buf, str, slen);
    buf[slen] = '\0';
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_seq_header(Int2DdsCdrReader *r, uint32_t *count_out) {
    if (!int2dds_cdr_read_u32(r, count_out)) return false;
    
    if ((size_t)(*count_out) > int2dds_cdr_reader_remaining(r)) {
        r->error = INT2DDS_CDR_ERR_UNDERFLOW;
        return false;
    }
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_bytes(Int2DdsCdrReader *r, uint8_t *out, size_t len) {
    if (!_int2dds_cdr_r_ok(r)) return false;
    if (!_int2dds_cdr_r_ensure(r, len)) return false;
    if (len > 0) memcpy(out, r->buf + r->pos, len);
    r->pos += len;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_prim_array(Int2DdsCdrReader *r, void *out,
                                                 size_t count, size_t elem_size) {
    if (!_int2dds_cdr_r_ok(r)) return false;
    if (!_int2dds_cdr_elem_size_ok(elem_size)) {
        r->error = INT2DDS_CDR_ERR_INVALID_ARGUMENT;
        return false;
    }
    if (count == 0) return true;
    if (count > (size_t)-1 / elem_size) {
        r->error = INT2DDS_CDR_ERR_UNDERFLOW;
        return false;
    }
    if (elem_size > 1 && !int2dds_cdr_read_align(r, elem_size)) return false;

    size_t n = count * elem_size;
    if (!_int2dds_cdr_r_ensure(r, n)) return false;
    if (elem_size == 1 || r->little_endian == INT2DDS_CDR_HOST_LE) {
        memcpy(out, r->buf + r->pos, n);
    } else {
        _int2dds_cdr_swap_copy((uint8_t *)out, r->buf + r->pos, count, elem_size);
    }
    r->pos += n;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_bool_array(Int2DdsCdrReader *r, bool *out, size_t count) {
    if (!_int2dds_cdr_r_ok(r)) return false;
    if (count == 0) return true;
    if (!_int2dds_cdr_r_ensure(r, count)) return false;

    /* Same normalization as int2dds_cdr_read_bool(): any nonzero octet reads as true. */
    uint8_t *dst = (uint8_t *)out;
    memcpy(dst, r->buf + r->pos, count);
    for (size_t i = 0; i < count; i++) dst[i] = dst[i] != 0 ? 1 : 0;
    r->pos += count;
    return true;
}

/* ---- XCDR2 DHEADER Read ----------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_read_dheader(Int2DdsCdrReader *r, uint32_t *object_size_out, size_t *start_pos_out) {
    if (!int2dds_cdr_read_u32(r, object_size_out)) {
        *start_pos_out = 0;
        return false;
    }
    *start_pos_out = r->pos;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_dheader_end(Int2DdsCdrReader *r, uint32_t object_size, size_t start_pos) {
    if (!_int2dds_cdr_r_ok(r)) return false;
    size_t expected_end = start_pos + object_size;
    if (expected_end > r->len) {
        r->error = INT2DDS_CDR_ERR_UNDERFLOW;
        return false;
    }
    /* Skip any remaining bytes (unknown trailing fields) */
    r->pos = expected_end;
    return true;
}

/* ---- XCDR2 EMHEADER Read ---------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_read_emheader(Int2DdsCdrReader *r, uint32_t *member_id_out, uint32_t *data_length_out, bool *must_understand_out) {
    uint32_t header;
    if (!int2dds_cdr_read_u32(r, &header)) {
        *member_id_out = 0; *data_length_out = 0; *must_understand_out = false;
        return false;
    }

    *must_understand_out = (header & 0x80000000u) != 0;
    uint8_t lc = (uint8_t)((header >> 28) & 0x07);
    *member_id_out = header & 0x0FFFFFFFu;

    switch (lc) {
    case 0: *data_length_out = 1; break;
    case 1: *data_length_out = 2; break;
    case 2: *data_length_out = 4; break;
    case 3: *data_length_out = 8; break;
    case 4: {
        uint32_t ext_len;
        if (!int2dds_cdr_read_u32(r, &ext_len)) { *data_length_out = 0; return false; }
        *data_length_out = ext_len;
        break;
    }
    case 5:
    case 6:
    case 7: {
        if (r->pos + 4 > r->len) {
            r->error = INT2DDS_CDR_ERR_UNDERFLOW;
            *data_length_out = 0;
            return false;
        }
        uint32_t nextint = _int2dds_get_u32(r->buf + r->pos, r->little_endian);
        if (lc == 5) {
            *data_length_out = nextint;
        } else if (lc == 6) {
            *data_length_out = 4u + 4u * nextint;
        } else {
            *data_length_out = 4u + 8u * nextint;
        }
        break;
    }
    default:
        *data_length_out = 0;
        break;
    }
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_is_sentinel(const Int2DdsCdrReader *r) {
    if (r->error != INT2DDS_CDR_OK) return false;
    if (r->pos + 4 > r->len) return false;

    uint32_t header = _int2dds_get_u32(r->buf + r->pos, r->little_endian);
    uint32_t member_id = header & 0x0FFFFFFFu;
    return member_id == INT2DDS_CDR_MEMBER_ID_SENTINEL;
}

INT2DDS_CDR_DEF bool int2dds_cdr_read_pid_header(Int2DdsCdrReader *r, uint32_t *member_id_out, uint32_t *data_length_out, bool *sentinel_out) {
    *member_id_out = 0;
    *data_length_out = 0;
    *sentinel_out = false;
    if (!int2dds_cdr_read_align(r, 4)) return false;
    if (!_int2dds_cdr_r_ensure(r, 4)) return false;
    uint16_t pid = _int2dds_get_u16(r->buf + r->pos, r->little_endian);
    uint16_t len = _int2dds_get_u16(r->buf + r->pos + 2, r->little_endian);
    r->pos += 4;
    uint16_t raw = pid & 0x3FFFu;
    if (raw == (INT2DDS_CDR_PID_SENTINEL & 0x3FFFu)) {
        *sentinel_out = true;
        return true;
    }
    if (raw == (INT2DDS_CDR_PID_EXTENDED & 0x3FFFu)) {
        if (!_int2dds_cdr_r_ensure(r, 8)) return false;
        *member_id_out = _int2dds_get_u32(r->buf + r->pos, r->little_endian);
        *data_length_out = _int2dds_get_u32(r->buf + r->pos + 4, r->little_endian);
        r->pos += 8;
        return true;
    }
    *member_id_out = raw;
    *data_length_out = len;
    return true;
}

INT2DDS_CDR_DEF bool int2dds_cdr_skip_bytes(Int2DdsCdrReader *r, size_t nbytes) {
    if (!_int2dds_cdr_r_ok(r)) return false;
    if (!_int2dds_cdr_r_ensure(r, nbytes)) return false;
    r->pos += nbytes;
    return true;
}

/* ---- Enum Read -------------------------------------------------------- */

INT2DDS_CDR_DEF bool int2dds_cdr_read_enum(Int2DdsCdrReader *r, int32_t *discriminant_out) {
    return int2dds_cdr_read_i32(r, discriminant_out);
}

#endif /* INT2DDS_CDR_IMPLEMENTATION || INT2DDS_CDR_STATIC */

#ifdef __cplusplus
}
#endif

#endif /* INT2DDS_CDR_H */
