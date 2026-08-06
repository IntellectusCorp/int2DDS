/**
 * @file test_cdr_bounds.c
 * @brief Bounds-check regression tests for int2dds_cdr.h (issue #378).
 *
 * Header-only mode, so this links against nothing. Build and run:
 *   cc -std=c99 -I../../include test_cdr_bounds.c -o test_cdr_bounds && ./test_cdr_bounds
 * or via the CMakeLists.txt beside this file.
 */

#define INT2DDS_CDR_STATIC
#include "int2dds_cdr.h"

#include <stdio.h>
#include <string.h>

static int g_failures = 0;

#define CHECK(cond, name)                                                      \
    do {                                                                       \
        if (cond) {                                                            \
            printf("  ok   %s\n", (name));                                     \
        } else {                                                               \
            printf("  FAIL %s  (%s:%d)\n", (name), __FILE__, __LINE__);        \
            g_failures++;                                                      \
        }                                                                      \
    } while (0)

/* `pos + n` wraps and passes an additive bounds check. Reachable from a wire
 * uint32 length only where size_t is 32 bits, but the arithmetic is broken on
 * every width, so a near-SIZE_MAX length reproduces it here. */
static void test_reader_length_overflow(void) {
    uint8_t buf[64];
    Int2DdsCdrReader r;
    uint8_t out[8];
    uint32_t tmp;

    memset(buf, 0, sizeof buf);
    int2dds_cdr_reader_init_no_header(&r, buf, sizeof buf, true, false);
    int2dds_cdr_read_u32(&r, &tmp);
    int2dds_cdr_read_u32(&r, &tmp);

    CHECK(int2dds_cdr_reader_position(&r) == 8, "reader advanced to 8");
    CHECK(!int2dds_cdr_read_bytes(&r, out, (size_t)-1), "read_bytes(SIZE_MAX) rejected");
    CHECK(int2dds_cdr_reader_error(&r) == INT2DDS_CDR_ERR_UNDERFLOW,
          "read_bytes(SIZE_MAX) reports underflow");
}

static void test_writer_length_overflow(void) {
    uint8_t buf[64];
    uint8_t data[8];
    Int2DdsCdrWriter w;

    memset(data, 0xAB, sizeof data);
    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, false);
    int2dds_cdr_write_u32(&w, 1);
    int2dds_cdr_write_u32(&w, 2);

    CHECK(!int2dds_cdr_write_bytes(&w, data, (size_t)-1), "write_bytes(SIZE_MAX) rejected");
    CHECK(int2dds_cdr_writer_error(&w) == INT2DDS_CDR_ERR_OVERFLOW,
          "write_bytes(SIZE_MAX) reports overflow");
}

/* The bulk APIs bound `count` with `count > SIZE_MAX / elem_size`, which is a
 * division by zero when elem_size is 0. */
static void test_bulk_elem_size(void) {
    uint8_t buf[64];
    uint8_t out[64];
    Int2DdsCdrReader r;
    Int2DdsCdrWriter w;

    memset(buf, 0, sizeof buf);

    int2dds_cdr_reader_init_no_header(&r, buf, sizeof buf, true, false);
    CHECK(!int2dds_cdr_read_prim_array(&r, out, 4, 0), "read_prim_array(elem_size=0) rejected");
    CHECK(int2dds_cdr_reader_error(&r) == INT2DDS_CDR_ERR_INVALID_ARGUMENT,
          "read_prim_array(elem_size=0) reports invalid argument");

    int2dds_cdr_reader_init_no_header(&r, buf, sizeof buf, true, false);
    CHECK(!int2dds_cdr_read_prim_array(&r, out, 4, 3), "read_prim_array(elem_size=3) rejected");

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, false);
    CHECK(!int2dds_cdr_write_prim_array(&w, out, 4, 0), "write_prim_array(elem_size=0) rejected");
    CHECK(int2dds_cdr_writer_error(&w) == INT2DDS_CDR_ERR_INVALID_ARGUMENT,
          "write_prim_array(elem_size=0) reports invalid argument");

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, false);
    CHECK(!int2dds_cdr_write_prim_array(&w, out, 4, 3), "write_prim_array(elem_size=3) rejected");
}

/* finalize() patches in place at `token` and derives a length from `pos - token`.
 * An out-of-range token is an out-of-bounds write on every pointer width; for
 * write_pid_finalize the underflowed length also drives an unbounded memmove. */
static void test_finalize_token(void) {
    uint8_t buf[64];
    Int2DdsCdrWriter w;
    size_t token;

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, true);
    int2dds_cdr_write_u32(&w, 0x11111111u);
    CHECK(!int2dds_cdr_write_dheader_finalize(&w, int2dds_cdr_writer_size(&w) + 16),
          "dheader_finalize(token > pos) rejected");
    CHECK(int2dds_cdr_writer_error(&w) == INT2DDS_CDR_ERR_INVALID_ARGUMENT,
          "dheader_finalize(token > pos) reports invalid argument");

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, true);
    int2dds_cdr_write_u32(&w, 0x11111111u);
    CHECK(!int2dds_cdr_write_dheader_finalize(&w, int2dds_cdr_writer_size(&w) - 2),
          "dheader_finalize(token too close to pos) rejected");

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, true);
    int2dds_cdr_write_u32(&w, 0x11111111u);
    CHECK(!int2dds_cdr_write_emheader_finalize(&w, int2dds_cdr_writer_size(&w) + 16),
          "emheader_finalize(token > pos) rejected");

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, false);
    int2dds_cdr_write_pid_begin(&w, 5, &token);
    int2dds_cdr_write_u32(&w, 0x22222222u);
    CHECK(!int2dds_cdr_write_pid_finalize(&w, 5, false, token + 1024),
          "pid_finalize(token > pos) rejected");
    CHECK(int2dds_cdr_writer_error(&w) == INT2DDS_CDR_ERR_INVALID_ARGUMENT,
          "pid_finalize(token > pos) reports invalid argument");
}

/* Wire path for the reader overflow: a uint32 length reaches _r_ensure without
 * being narrowed. This asserts a clean rejection; it only distinguishes fixed
 * from broken when size_t is 32 bits (see #381 for the 32-bit CI job). */
static void test_read_string_wire_length(void) {
    uint8_t buf[16];
    Int2DdsCdrReader r;
    const char *s = (const char *)0x1;
    size_t n = 12345;

    memset(buf, 0x41, sizeof buf);
    buf[0] = 0xF0; buf[1] = 0xFF; buf[2] = 0xFF; buf[3] = 0xFF; /* LE 0xFFFFFFF0 */

    int2dds_cdr_reader_init_no_header(&r, buf, sizeof buf, true, false);
    CHECK(!int2dds_cdr_read_string(&r, &s, &n), "read_string(oversized length) rejected");
    CHECK(s == NULL && n == 0, "read_string zeroes its outputs on failure");
    CHECK(int2dds_cdr_reader_error(&r) == INT2DDS_CDR_ERR_UNDERFLOW,
          "read_string(oversized length) reports underflow");
}

/* The new checks must not reject valid usage. */
static void test_valid_paths_still_work(void) {
    uint8_t buf[128];
    uint8_t out[128];
    Int2DdsCdrWriter w;
    Int2DdsCdrReader r;
    size_t token;
    uint32_t vals[4] = { 1, 2, 3, 4 };
    uint32_t got[4] = { 0, 0, 0, 0 };
    uint32_t dheader = 0;

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, true);
    CHECK(int2dds_cdr_write_dheader_begin(&w, &token), "dheader_begin");
    CHECK(int2dds_cdr_write_prim_array(&w, vals, 4, 4), "write_prim_array(4x4)");
    CHECK(int2dds_cdr_write_dheader_finalize(&w, token), "dheader_finalize(valid token)");
    CHECK(int2dds_cdr_writer_error(&w) == INT2DDS_CDR_OK, "writer stayed clean");

    int2dds_cdr_reader_init_no_header(&r, buf, int2dds_cdr_writer_size(&w), true, true);
    CHECK(int2dds_cdr_read_u32(&r, &dheader), "read dheader");
    CHECK(dheader == 16, "dheader length is 16");
    CHECK(int2dds_cdr_read_prim_array(&r, got, 4, 4), "read_prim_array(4x4)");
    CHECK(memcmp(vals, got, sizeof vals) == 0, "prim_array round-trips");

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, true);
    CHECK(int2dds_cdr_write_emheader_begin(&w, 7, false, &token), "emheader_begin");
    CHECK(int2dds_cdr_write_u32(&w, 0xDEADBEEFu), "emheader body");
    CHECK(int2dds_cdr_write_emheader_finalize(&w, token), "emheader_finalize(valid token)");
    CHECK(int2dds_cdr_writer_error(&w) == INT2DDS_CDR_OK, "emheader writer stayed clean");

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, false);
    CHECK(int2dds_cdr_write_pid_begin(&w, 5, &token), "pid_begin(short)");
    CHECK(int2dds_cdr_write_u32(&w, 0xCAFEBABEu), "pid body");
    CHECK(int2dds_cdr_write_pid_finalize(&w, 5, false, token), "pid_finalize(valid token)");
    CHECK(int2dds_cdr_writer_error(&w) == INT2DDS_CDR_OK, "pid writer stayed clean");

    int2dds_cdr_writer_init(&w, buf, sizeof buf, true, false);
    CHECK(int2dds_cdr_write_string(&w, "hello"), "write_string");
    int2dds_cdr_reader_init_no_header(&r, buf, int2dds_cdr_writer_size(&w), true, false);
    {
        const char *s = NULL;
        size_t n = 0;
        CHECK(int2dds_cdr_read_string(&r, &s, &n), "read_string(valid)");
        CHECK(n == 5 && memcmp(s, "hello", 5) == 0, "string round-trips");
    }
    (void)out;
}

int main(void) {
    printf("test_cdr_bounds\n");
    test_reader_length_overflow();
    test_writer_length_overflow();
    test_bulk_elem_size();
    test_finalize_token();
    test_read_string_wire_length();
    test_valid_paths_still_work();

    if (g_failures == 0) {
        printf("all checks passed\n");
        return 0;
    }
    printf("%d check(s) failed\n", g_failures);
    return 1;
}
