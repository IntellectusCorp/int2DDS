/*
 * int2dds FFI Performance Test Subscriber
 *
 * Uses IDL-generated CDR serialization (LatencyTestData.h, PerformanceTestData.h)
 * with int2dds_write_serialized / int2dds_take_serialized API.
 *
 * Three test modes:
 *   - throughput:     Measures received throughput, tracks lost samples
 *   - latency:        Echo mode - echoes back latency pings to publisher
 *   - local_latency:  Measures one-way latency (receive_time - send_timestamp)
 *
 * Arguments match the Rust perftest_subscriber exactly.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <signal.h>
#include <stdint.h>
#include <stdbool.h>
#include <inttypes.h>
#include <math.h>

#ifdef _WIN32
#include <windows.h>
#define sleep_ms(ms) Sleep(ms)

static uint64_t get_monotonic_ns(void) {
    static LARGE_INTEGER freq = {0};
    LARGE_INTEGER counter;
    if (freq.QuadPart == 0) QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&counter);
    return (uint64_t)((double)counter.QuadPart * 1000000000.0 / (double)freq.QuadPart);
}

#else
#include <unistd.h>
#define sleep_ms(ms) usleep((ms) * 1000)

static uint64_t get_monotonic_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}
#endif

#define INT2DDS_CDR_STATIC
#include "int2dds-ffi.h"
#include "LatencyTestData.h"
#include "PerformanceTestData.h"

/* ====== CDR Layout Offsets (FINAL, XCDR2 LE) ======
 *
 * PerformanceTestData:
 *   [0..3]   encapsulation header
 *   [4..11]  seq_num (u64)
 *   [12..19] timestamp (u64)
 *   [20..23] data.length (u32)
 *   [24..]   data bytes
 *
 * LatencyTestData:
 *   [0..3]   encapsulation header
 *   [4..11]  seq_num (u64)
 *   [12..19] send_timestamp (u64)
 *   [20..27] echo_timestamp (u64)
 *   [28..31] data.length (u32)
 *   [32..]   data bytes
 */
#define PERFDATA_CDR_SEQNUM_OFFSET     4
#define PERFDATA_CDR_TIMESTAMP_OFFSET  12
#define PERFDATA_CDR_DATALEN_OFFSET    20

#define LATDATA_CDR_SEQNUM_OFFSET      4
#define LATDATA_CDR_SEND_TS_OFFSET     12
#define LATDATA_CDR_ECHO_TS_OFFSET     20
#define LATDATA_CDR_DATALEN_OFFSET     28

/* ====== Timing ====== */

static uint64_t get_current_time_ns(void) {
#ifdef _WIN32
    FILETIME ft;
    ULARGE_INTEGER uli;
    GetSystemTimePreciseAsFileTime(&ft);
    uli.LowPart = ft.dwLowDateTime;
    uli.HighPart = ft.dwHighDateTime;
    uli.QuadPart -= 116444736000000000ULL;
    return uli.QuadPart * 100ULL;
#else
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
#endif
}

/* ====== Global State ====== */

static volatile bool g_running = true;

static void signal_handler(int sig) {
    (void)sig;
    g_running = false;
}

/* ====== Simple HashSet for out-of-order sequence tracking ======
 * Open-addressing hash set with linear probing.
 */

typedef struct {
    uint64_t *buckets;
    bool     *occupied;
    uint32_t  capacity;
    uint32_t  count;
} seqnum_set_t;

static void seqnum_set_init(seqnum_set_t *s, uint32_t initial_cap) {
    s->capacity = initial_cap > 0 ? initial_cap : 65536;
    s->buckets = (uint64_t*)calloc(s->capacity, sizeof(uint64_t));
    s->occupied = (bool*)calloc(s->capacity, sizeof(bool));
    s->count = 0;
}

static void seqnum_set_destroy(seqnum_set_t *s) {
    free(s->buckets);
    free(s->occupied);
    s->buckets = NULL;
    s->occupied = NULL;
    s->count = 0;
    s->capacity = 0;
}

static void seqnum_set_grow(seqnum_set_t *s);

/* Returns true if the value was newly inserted, false if already present. */
static bool seqnum_set_insert(seqnum_set_t *s, uint64_t val) {
    if (s->count * 4 >= s->capacity * 3) {
        seqnum_set_grow(s);
    }
    uint32_t idx = (uint32_t)(val % s->capacity);
    while (s->occupied[idx]) {
        if (s->buckets[idx] == val) return false;
        idx = (idx + 1) % s->capacity;
    }
    s->buckets[idx] = val;
    s->occupied[idx] = true;
    s->count++;
    return true;
}

static void seqnum_set_grow(seqnum_set_t *s) {
    uint32_t old_cap = s->capacity;
    uint64_t *old_buckets = s->buckets;
    bool *old_occupied = s->occupied;

    s->capacity = old_cap * 2;
    s->buckets = (uint64_t*)calloc(s->capacity, sizeof(uint64_t));
    s->occupied = (bool*)calloc(s->capacity, sizeof(bool));
    s->count = 0;

    for (uint32_t i = 0; i < old_cap; i++) {
        if (old_occupied[i]) {
            seqnum_set_insert(s, old_buckets[i]);
        }
    }
    free(old_buckets);
    free(old_occupied);
}

static uint64_t seqnum_set_max(const seqnum_set_t *s) {
    uint64_t max_val = 0;
    for (uint32_t i = 0; i < s->capacity; i++) {
        if (s->occupied[i] && s->buckets[i] > max_val) {
            max_val = s->buckets[i];
        }
    }
    return max_val;
}

/* ====== Latency sample collector ====== */

typedef struct {
    double  *samples;
    uint32_t count;
    uint32_t capacity;
} latency_collector_t;

static void latency_collector_init(latency_collector_t *c, uint32_t initial_cap) {
    c->capacity = initial_cap > 0 ? initial_cap : 65536;
    c->samples = (double*)malloc(c->capacity * sizeof(double));
    c->count = 0;
}

static void latency_collector_destroy(latency_collector_t *c) {
    free(c->samples);
    c->samples = NULL;
    c->count = 0;
    c->capacity = 0;
}

static void latency_collector_add(latency_collector_t *c, double val) {
    if (c->count >= c->capacity) {
        c->capacity *= 2;
        c->samples = (double*)realloc(c->samples, c->capacity * sizeof(double));
    }
    c->samples[c->count++] = val;
}

static int double_compare(const void *a, const void *b) {
    double da = *(const double*)a;
    double db = *(const double*)b;
    if (da < db) return -1;
    if (da > db) return 1;
    return 0;
}

/* ====== Throughput Subscriber Context ====== */

typedef struct {
    uint8_t        *recv_buf;
    size_t          buf_capacity;

    /* Stats */
    uint64_t        total_samples;
    uint64_t        bytes_received;
    uint64_t        lost_samples;
    uint64_t        expected_seq;
    bool            out_of_order;
    seqnum_set_t    seen_seqs;

    /* Timing (monotonic, relative to start_mono) */
    uint64_t        start_mono;
    volatile uint64_t last_message_mono;
    volatile bool   first_sample_received;
    volatile bool   should_stop;
} throughput_ctx_t;

static throughput_ctx_t g_thr_ctx;

/* ====== Throughput: on_data_available callback ====== */

static void on_throughput_data_available(Int2DdsDataReader *reader, Int2DdsUserContext ctx) {
    (void)ctx;
    uint64_t now_mono = get_monotonic_ns();

    if (g_thr_ctx.should_stop) return;

    uintptr_t actual_size;
    bool valid_data;

    while (1) {
        Int2DdsRet ret = int2dds_take_serialized(reader, g_thr_ctx.recv_buf,
                                                  g_thr_ctx.buf_capacity,
                                                  &actual_size, &valid_data);
        if (ret == INT2DDS_RET_NO_DATA) break;
        if (ret != INT2DDS_RET_OK || !valid_data) continue;

        /* Update last message time */
        g_thr_ctx.last_message_mono = now_mono;

        /* Read seq_num from CDR offset */
        uint64_t seq_num;
        memcpy(&seq_num, g_thr_ctx.recv_buf + PERFDATA_CDR_SEQNUM_OFFSET, sizeof(uint64_t));

        /* Read data.length from CDR offset for bytes tracking */
        uint32_t data_len;
        memcpy(&data_len, g_thr_ctx.recv_buf + PERFDATA_CDR_DATALEN_OFFSET, sizeof(uint32_t));

        if (!g_thr_ctx.first_sample_received) {
            g_thr_ctx.first_sample_received = true;
            g_thr_ctx.start_mono = now_mono;
            printf("First data sample received. Starting measurements...\n");
        }

        if (g_thr_ctx.out_of_order) {
            /* Out-of-order mode: insert into set, count only new entries */
            if (seqnum_set_insert(&g_thr_ctx.seen_seqs, seq_num)) {
                g_thr_ctx.total_samples++;
                g_thr_ctx.bytes_received += data_len;
            }
        } else {
            /* In-order mode: track expected_seq and detect lost samples */
            seqnum_set_insert(&g_thr_ctx.seen_seqs, seq_num);
            if (seq_num >= g_thr_ctx.expected_seq) {
                g_thr_ctx.lost_samples += seq_num - g_thr_ctx.expected_seq;
                g_thr_ctx.expected_seq = seq_num + 1;
                g_thr_ctx.total_samples++;
                g_thr_ctx.bytes_received += data_len;
            }
            /* Skip duplicates / out-of-order */
        }
    }
}

static void on_throughput_subscription_matched(
    Int2DdsDataReader *reader,
    const Int2DdsSubscriptionMatchedStatus *status,
    Int2DdsUserContext ctx)
{
    (void)reader; (void)ctx;
    if (status->current_count > 0) {
        printf("Publisher matched! Waiting for data...\n");
    }
}

/* ====== Latency Echo Subscriber Context ====== */

typedef struct {
    uint8_t           *recv_buf;
    size_t             buf_capacity;
    Int2DdsDataWriter  *echo_writer;

    /* Stats */
    uint64_t           total_samples;
    uint64_t           start_mono;
    volatile bool      first_sample_received;

    /* Periodic reporting */
    uint64_t           last_report_mono;
} latency_echo_ctx_t;

static latency_echo_ctx_t g_lat_ctx;

/* ====== Latency Echo: on_data_available callback ======
 * Zero-copy echo: read CDR buffer, set echo_timestamp in-place, write back.
 */

static void on_latency_data_available(Int2DdsDataReader *reader, Int2DdsUserContext ctx) {
    (void)ctx;
    uint64_t current_time = get_current_time_ns();
    uint64_t now_mono = get_monotonic_ns();

    uintptr_t actual_size;
    bool valid_data;

    while (1) {
        Int2DdsRet ret = int2dds_take_serialized(reader, g_lat_ctx.recv_buf,
                                                  g_lat_ctx.buf_capacity,
                                                  &actual_size, &valid_data);
        if (ret == INT2DDS_RET_NO_DATA) break;
        if (ret != INT2DDS_RET_OK || !valid_data) continue;

        if (!g_lat_ctx.first_sample_received) {
            g_lat_ctx.first_sample_received = true;
            g_lat_ctx.start_mono = now_mono;
            printf("First latency sample received. Starting measurements...\n");
        }

        g_lat_ctx.total_samples++;

        /* Check echo_timestamp: if 0, this is a ping from publisher -> echo back */
        uint64_t echo_ts;
        memcpy(&echo_ts, g_lat_ctx.recv_buf + LATDATA_CDR_ECHO_TS_OFFSET, sizeof(uint64_t));

        if (echo_ts == 0) {
            /* Zero-copy echo: set echo_timestamp in CDR buffer, write back */
            memcpy(g_lat_ctx.recv_buf + LATDATA_CDR_ECHO_TS_OFFSET, &current_time, sizeof(uint64_t));

            if (g_lat_ctx.echo_writer) {
                Int2DdsRet wr_ret = int2dds_write_serialized(
                    g_lat_ctx.echo_writer, g_lat_ctx.recv_buf, actual_size, NULL, 0);
                if (wr_ret != INT2DDS_RET_OK) {
                    fprintf(stderr, "Failed to echo latency sample: %d\n", wr_ret);
                }
            }
        }

        /* 5-second periodic reporting */
        if (now_mono - g_lat_ctx.last_report_mono >= 5000000000ULL) {
            printf("Echo mode: responding to latency pings\n");
            g_lat_ctx.last_report_mono = now_mono;
        }
    }
}

static void on_latency_subscription_matched(
    Int2DdsDataReader *reader,
    const Int2DdsSubscriptionMatchedStatus *status,
    Int2DdsUserContext ctx)
{
    (void)reader; (void)ctx;
    if (status->current_count > 0) {
        printf("Latency test publisher matched! Waiting for data...\n");
    }
}

/* ====== Local Latency Subscriber Context ====== */

typedef struct {
    uint8_t              *recv_buf;
    size_t                buf_capacity;

    /* Stats */
    uint64_t              total_samples;
    uint64_t              bytes_received;
    latency_collector_t   latency_samples;
    seqnum_set_t          seen_seqs;

    /* Timing */
    uint64_t              start_mono;
    volatile uint64_t     last_message_mono;
    volatile bool         first_sample_received;
    volatile bool         should_stop;
} local_latency_ctx_t;

static local_latency_ctx_t g_ll_ctx;

/* ====== Local Latency: on_data_available callback ====== */

static void on_local_latency_data_available(Int2DdsDataReader *reader, Int2DdsUserContext ctx) {
    (void)ctx;
    uint64_t receive_time_ns = get_current_time_ns();
    uint64_t now_mono = get_monotonic_ns();

    if (g_ll_ctx.should_stop) return;

    uintptr_t actual_size;
    bool valid_data;

    while (1) {
        Int2DdsRet ret = int2dds_take_serialized(reader, g_ll_ctx.recv_buf,
                                                  g_ll_ctx.buf_capacity,
                                                  &actual_size, &valid_data);
        if (ret == INT2DDS_RET_NO_DATA) break;
        if (ret != INT2DDS_RET_OK || !valid_data) continue;

        /* Update last message time */
        g_ll_ctx.last_message_mono = now_mono;

        /* Read seq_num from CDR offset */
        uint64_t seq_num;
        memcpy(&seq_num, g_ll_ctx.recv_buf + PERFDATA_CDR_SEQNUM_OFFSET, sizeof(uint64_t));

        /* Read timestamp (send_timestamp) from CDR offset */
        uint64_t send_ts;
        memcpy(&send_ts, g_ll_ctx.recv_buf + PERFDATA_CDR_TIMESTAMP_OFFSET, sizeof(uint64_t));

        /* Read data.length */
        uint32_t data_len;
        memcpy(&data_len, g_ll_ctx.recv_buf + PERFDATA_CDR_DATALEN_OFFSET, sizeof(uint32_t));

        if (!g_ll_ctx.first_sample_received) {
            g_ll_ctx.first_sample_received = true;
            g_ll_ctx.start_mono = now_mono;
            printf("First local latency sample received. Starting measurements...\n");
        }

        g_ll_ctx.total_samples++;
        g_ll_ctx.bytes_received += data_len;
        seqnum_set_insert(&g_ll_ctx.seen_seqs, seq_num);

        /* One-way latency = receive_time - send_timestamp */
        double latency_ns = (double)(receive_time_ns - send_ts);
        latency_collector_add(&g_ll_ctx.latency_samples, latency_ns);
    }
}

static void on_local_latency_subscription_matched(
    Int2DdsDataReader *reader,
    const Int2DdsSubscriptionMatchedStatus *status,
    Int2DdsUserContext ctx)
{
    (void)reader; (void)ctx;
    if (status->current_count > 0) {
        printf("Local latency test publisher matched! Waiting for data...\n");
    }
}

/* ====== Command-line Arguments ====== */

typedef struct {
    char     test_mode[32];
    int32_t  domain_id;
    size_t   data_len;
    uint64_t execution_time;
    char     reliability[32];
    double   hz;
    uint64_t warmup_time;
    bool     out_of_order;
} subscriber_args_t;

static void print_usage(const char *prog) {
    printf("Usage: %s [OPTIONS]\n", prog);
    printf("  --test-mode MODE       throughput|latency|local_latency (default: throughput)\n");
    printf("  --domain-id ID         Domain ID (default: 10)\n");
    printf("  --data-len SIZE        Data size in bytes (default: 1024)\n");
    printf("  --execution-time SEC   Test duration in seconds (default: 30)\n");
    printf("  --reliability MODE     best|besteffort|best_effort|reliable (default: best)\n");
    printf("  --hz RATE              Rate limit in Hz (optional, for CSV naming)\n");
    printf("  --warmup-time SEC      Warmup sleep in seconds (default: 1)\n");
    printf("  --out-of-order BOOL    Enable out-of-order tracking (default: true)\n");
}

/* Match argument key, supporting both --key=value and --key value forms,
 * and treating hyphens and underscores as equivalent (e.g. --test-mode = --test_mode).
 * If matched, returns the value string and advances *index past the consumed args.
 * Returns NULL if no match. */
static const char* match_arg(int argc, char *argv[], int *index, const char *key) {
    const char *arg = argv[*index];
    size_t key_len = strlen(key);

    /* Try exact match or underscore/hyphen normalized match */
    bool key_match = true;
    if (strncmp(arg, key, key_len) == 0) {
        /* exact prefix match */
    } else {
        /* normalized comparison: treat '-' and '_' as equivalent */
        key_match = true;
        for (size_t j = 0; j < key_len; j++) {
            char a = arg[j];
            char k = key[j];
            if (a == '_') a = '-';
            if (k == '_') k = '-';
            if (a != k) { key_match = false; break; }
        }
    }

    if (!key_match) return NULL;

    /* Check for --key=value form */
    if (arg[key_len] == '=') {
        return arg + key_len + 1;
    }

    /* Check for --key value form (exact key match, nothing after) */
    if (arg[key_len] == '\0' && *index + 1 < argc) {
        (*index)++;
        return argv[*index];
    }

    return NULL;
}

static void parse_args(int argc, char *argv[], subscriber_args_t *args) {
    strcpy(args->test_mode, "throughput");
    args->domain_id = 10;
    args->data_len = 1024;
    args->execution_time = 30;
    strcpy(args->reliability, "best");
    args->hz = 0.0;
    args->warmup_time = 1;
    args->out_of_order = true;

    for (int i = 1; i < argc; i++) {
        const char *val;
        if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "-h") == 0) {
            print_usage(argv[0]);
            exit(0);
        } else if ((val = match_arg(argc, argv, &i, "--test-mode")) != NULL) {
            strncpy(args->test_mode, val, sizeof(args->test_mode) - 1);
            args->test_mode[sizeof(args->test_mode) - 1] = '\0';
        } else if ((val = match_arg(argc, argv, &i, "--domain-id")) != NULL) {
            args->domain_id = (int32_t)atoi(val);
        } else if ((val = match_arg(argc, argv, &i, "--data-len")) != NULL) {
            args->data_len = (size_t)strtoull(val, NULL, 10);
        } else if ((val = match_arg(argc, argv, &i, "--execution-time")) != NULL) {
            args->execution_time = (uint64_t)strtoull(val, NULL, 10);
        } else if ((val = match_arg(argc, argv, &i, "--reliability")) != NULL) {
            strncpy(args->reliability, val, sizeof(args->reliability) - 1);
            args->reliability[sizeof(args->reliability) - 1] = '\0';
        } else if ((val = match_arg(argc, argv, &i, "--hz")) != NULL) {
            args->hz = atof(val);
        } else if ((val = match_arg(argc, argv, &i, "--warmup-time")) != NULL) {
            args->warmup_time = (uint64_t)strtoull(val, NULL, 10);
        } else if ((val = match_arg(argc, argv, &i, "--out-of-order")) != NULL) {
            args->out_of_order = (strcmp(val, "true") == 0 || strcmp(val, "1") == 0);
        } else {
            fprintf(stderr, "Warning: unknown argument '%s'\n", argv[i]);
        }
    }
}

static int is_reliable(const char *reliability) {
    return strcmp(reliability, "reliable") == 0;
}

static const char* reliability_str(const char *reliability) {
    return is_reliable(reliability) ? "reliable" : "best";
}

/* ====== CSV / Log timestamp helpers ====== */

static void get_timestamp_str(char *buf, size_t size) {
    time_t now = time(NULL);
    struct tm *tm_info = localtime(&now);
    strftime(buf, size, "%Y-%m-%d %H:%M:%S", tm_info);
}

static void get_log_timestamp_str(char *buf, size_t size) {
    time_t now = time(NULL);
    struct tm *tm_info = localtime(&now);
    strftime(buf, size, "%Y%m%dT%H%M%S", tm_info);
}

/* ================================================================
 * THROUGHPUT SUBSCRIBER
 * ================================================================ */

static void run_throughput_subscriber(const subscriber_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsSubscriber *subscriber = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsDataReader *reader = NULL;
    Int2DdsDataReaderQos *qos = NULL;

    printf("Starting throughput subscriber:\n");
    printf("  Reliability: %s\n", reliability_str(args->reliability));
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    printf("  Out-of-order mode: %s\n", args->out_of_order ? "true" : "false");
    printf("\n");

    /* Init throughput context */
    memset(&g_thr_ctx, 0, sizeof(g_thr_ctx));
    g_thr_ctx.buf_capacity = 256 + args->data_len;
    g_thr_ctx.recv_buf = (uint8_t*)malloc(g_thr_ctx.buf_capacity);
    if (!g_thr_ctx.recv_buf) { fprintf(stderr, "Allocation failed\n"); return; }
    g_thr_ctx.out_of_order = args->out_of_order;
    seqnum_set_init(&g_thr_ctx.seen_seqs, 65536);

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to get factory: %d\n", ret); goto cleanup; }

    ret = int2dds_create_participant(factory, "perftest_subscriber", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create participant: %d\n", ret); goto cleanup; }

    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create subscriber: %d\n", ret); goto cleanup; }

    ret = int2dds_create_topic(participant, "throughput_test_topic", "PerformanceTestData",
                                0 /* FINAL */, NULL, &topic);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create topic: %d\n", ret); goto cleanup; }

    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) goto cleanup;
    int2dds_datareader_qos_set_reliability(qos,
        is_reliable(args->reliability) ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT);
    int2dds_datareader_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 100);

    Int2DdsDataReaderListener rd_listener = {
        .on_data_available = on_throughput_data_available,
        .on_subscription_matched = on_throughput_subscription_matched,
        .on_sample_rejected = NULL, .on_liveliness_changed = NULL,
        .on_requested_deadline_missed = NULL, .on_requested_incompatible_qos = NULL,
        .on_sample_lost = NULL, .user_context = NULL
    };
    ret = int2dds_create_datareader_with_listener(subscriber, topic, qos, &rd_listener,
                                                   INT2DDS_STATUS_DATA_AVAILABLE | INT2DDS_STATUS_SUBSCRIPTION_MATCHED,
                                                   &reader);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datareader: %d\n", ret); goto cleanup; }

    printf("Waiting for publisher...\n");

    /* Wait for first sample */
    while (g_running && !g_thr_ctx.first_sample_received) {
        sleep_ms(100);
    }
    if (!g_running) goto cleanup;

    /* 5-second periodic reporting */
    uint64_t last_report_mono = g_thr_ctx.start_mono;
    uint64_t last_report_samples = 0;
    uint64_t report_interval = 0;

    while (g_running && !g_thr_ctx.should_stop) {
        sleep_ms(100);

        /* Check 2-second timeout after last message */
        if (g_thr_ctx.last_message_mono > 0) {
            uint64_t now_mono = get_monotonic_ns();
            if (now_mono - g_thr_ctx.last_message_mono >= 2000000000ULL) {
                printf("\n2 second timeout after last message. Stopping...\n");
                g_thr_ctx.should_stop = true;
                break;
            }
        }

        /* 5-second reporting */
        uint64_t now_mono = get_monotonic_ns();
        double elapsed_since_start = (double)(now_mono - g_thr_ctx.start_mono) / 1e9;
        report_interval++;

        /* Check every ~5 seconds (50 * 100ms) */
        if (report_interval >= 50) {
            report_interval = 0;

            if (g_thr_ctx.total_samples > 0) {
                double report_elapsed = (double)(now_mono - last_report_mono) / 1e9;
                double duration = elapsed_since_start;

                /* Calculate stats snapshot */
                uint64_t total = g_thr_ctx.total_samples;
                uint64_t lost = g_thr_ctx.lost_samples;

                /* In out-of-order mode, recalculate lost from set */
                if (g_thr_ctx.out_of_order) {
                    uint64_t max_seq = seqnum_set_max(&g_thr_ctx.seen_seqs);
                    lost = (max_seq + 1) - g_thr_ctx.seen_seqs.count;
                }

                double msgs_per_sec = total / duration;
                double mbps = (g_thr_ctx.bytes_received * 8.0) / (duration * 1e6);

                uint64_t max_seq = seqnum_set_max(&g_thr_ctx.seen_seqs);
                double loss_rate = max_seq > 0 ? (lost * 100.0 / max_seq) : 0.0;

                printf("[%.0fs] Received: %" PRIu64 ", Rate: %.0f msg/s, %.2f Mbps, "
                       "Lost: %" PRIu64 ", Last seq: %" PRIu64 ", Loss rate: %.2f%%\n",
                       duration, total, msgs_per_sec, mbps, lost, max_seq, loss_rate);

                last_report_mono = now_mono;
                last_report_samples = total;
            }
        }
    }

    /* Final stats */
    {
        uint64_t end_mono = g_thr_ctx.last_message_mono > 0 ? g_thr_ctx.last_message_mono : get_monotonic_ns();
        double duration = (double)(end_mono - g_thr_ctx.start_mono) / 1e9;

        uint64_t total = g_thr_ctx.total_samples;
        uint64_t lost = g_thr_ctx.lost_samples;

        if (g_thr_ctx.out_of_order) {
            uint64_t max_seq = seqnum_set_max(&g_thr_ctx.seen_seqs);
            if (max_seq > 0) {
                lost = (max_seq + 1) - g_thr_ctx.seen_seqs.count;
            }
        }

        double msgs_per_sec = duration > 0 ? total / duration : 0;
        double mbps = duration > 0 ? (g_thr_ctx.bytes_received * 8.0) / (duration * 1e6) : 0;

        printf("\n=== Performance Test Results ===\n");
        printf("Test duration: %.2f seconds\n", duration);
        printf("[INFO] Total samples received: %" PRIu64 "\n", total);
        printf("[INFO] Messages per second: %.2f\n", msgs_per_sec);
        printf("Throughput: %.2f Mbps\n", mbps);

        uint64_t max_seq = seqnum_set_max(&g_thr_ctx.seen_seqs);
        if (max_seq > 0) {
            double loss_rate = (lost * 100.0) / max_seq;
            printf("Last sequence number: %" PRIu64 "\n", max_seq);
            if (lost > 0) printf("[INFO] Lost samples: %" PRIu64 "\n", lost);
            printf("[INFO] Loss rate: %.2f%%\n", loss_rate);
        }

        /* Log file */
        char ts_str[64]; get_log_timestamp_str(ts_str, sizeof(ts_str));
        char log_fn[256]; snprintf(log_fn, sizeof(log_fn), "sub_thr-%s.log", ts_str);
        FILE *logf = fopen(log_fn, "w");
        if (logf) {
            fprintf(logf, "Total received: %" PRIu64 "\nLost samples: %" PRIu64 "\n"
                    "Rate: %.2f msg/s\nThroughput: %.2f Mbps\n",
                    total, lost, msgs_per_sec, mbps);
            fclose(logf);
        }

        /* CSV */
        char csv_fn[256];
        if (args->hz > 0.0)
            snprintf(csv_fn, sizeof(csv_fn), "subscriber_throughput_results_%zub_%.0fhz.csv", args->data_len, args->hz);
        else
            snprintf(csv_fn, sizeof(csv_fn), "subscriber_throughput_results_%zub.csv", args->data_len);

        bool csv_exists = false;
        { FILE *test = fopen(csv_fn, "r"); if (test) { csv_exists = true; fclose(test); } }
        FILE *csv = fopen(csv_fn, "a");
        if (csv) {
            if (!csv_exists)
                fprintf(csv, "Timestamp,Total_Received_Messages,Messages_Per_Second,Lost_Messages,Loss_Rate_Percent,Throughput_Mbps\n");
            char csv_ts[64]; get_timestamp_str(csv_ts, sizeof(csv_ts));
            double loss_rate = max_seq > 0 ? (lost * 100.0 / max_seq) : 0.0;
            fprintf(csv, "%s,%" PRIu64 ",%.2f,%" PRIu64 ",%.2f,%.2f\n",
                    csv_ts, total, msgs_per_sec, lost, loss_rate, mbps);
            fclose(csv);
        }
    }

cleanup:
    seqnum_set_destroy(&g_thr_ctx.seen_seqs);
    free(g_thr_ctx.recv_buf);
    g_thr_ctx.recv_buf = NULL;
    if (qos) int2dds_datareader_qos_destroy(qos);
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ================================================================
 * LATENCY ECHO SUBSCRIBER
 * ================================================================ */

static void run_latency_subscriber(const subscriber_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsSubscriber *subscriber = NULL;
    Int2DdsPublisher *publisher = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsTopic *echo_topic = NULL;
    Int2DdsDataReader *reader = NULL;
    Int2DdsDataWriter *echo_writer = NULL;
    Int2DdsDataReaderQos *reader_qos = NULL;
    Int2DdsDataWriterQos *writer_qos = NULL;

    printf("Starting latency subscriber (echo mode):\n");
    printf("  Reliability: %s\n", reliability_str(args->reliability));
    printf("  Execution time: %" PRIu64 " seconds (from first sample)\n", args->execution_time);
    printf("\n");

    /* Init latency echo context */
    memset(&g_lat_ctx, 0, sizeof(g_lat_ctx));
    g_lat_ctx.buf_capacity = 256 + args->data_len;
    g_lat_ctx.recv_buf = (uint8_t*)malloc(g_lat_ctx.buf_capacity);
    if (!g_lat_ctx.recv_buf) { fprintf(stderr, "Allocation failed\n"); return; }
    g_lat_ctx.last_report_mono = get_monotonic_ns();

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to get factory: %d\n", ret); goto cleanup; }

    ret = int2dds_create_participant(factory, "perftest_latency_subscriber", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create participant: %d\n", ret); goto cleanup; }

    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create subscriber: %d\n", ret); goto cleanup; }

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create publisher: %d\n", ret); goto cleanup; }

    ret = int2dds_create_topic(participant, "latency_test_topic", "LatencyTestData",
                                0 /* FINAL */, NULL, &topic);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create topic: %d\n", ret); goto cleanup; }

    ret = int2dds_create_topic(participant, "latency_test_topic_echo", "LatencyTestData",
                                0 /* FINAL */, NULL, &echo_topic);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create echo topic: %d\n", ret); goto cleanup; }

    /* Reader QoS */
    ret = int2dds_datareader_qos_create_default(&reader_qos);
    if (ret != INT2DDS_RET_OK) goto cleanup;
    int2dds_datareader_qos_set_reliability(reader_qos,
        is_reliable(args->reliability) ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT);

    /* Writer QoS */
    ret = int2dds_datawriter_qos_create_default(&writer_qos);
    if (ret != INT2DDS_RET_OK) goto cleanup;
    int2dds_datawriter_qos_set_reliability(writer_qos,
        is_reliable(args->reliability) ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        100000000);
    int2dds_datawriter_qos_set_history(writer_qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 100);

    /* Create echo writer first (needed by listener callback) */
    ret = int2dds_create_datawriter(publisher, echo_topic, writer_qos, &echo_writer);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create echo writer: %d\n", ret); goto cleanup; }
    g_lat_ctx.echo_writer = echo_writer;

    /* Create reader with listener */
    Int2DdsDataReaderListener rd_listener = {
        .on_data_available = on_latency_data_available,
        .on_subscription_matched = on_latency_subscription_matched,
        .on_sample_rejected = NULL, .on_liveliness_changed = NULL,
        .on_requested_deadline_missed = NULL, .on_requested_incompatible_qos = NULL,
        .on_sample_lost = NULL, .user_context = NULL
    };
    ret = int2dds_create_datareader_with_listener(subscriber, topic, reader_qos, &rd_listener,
                                                   INT2DDS_STATUS_DATA_AVAILABLE | INT2DDS_STATUS_SUBSCRIPTION_MATCHED,
                                                   &reader);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datareader: %d\n", ret); goto cleanup; }

    printf("Waiting for latency test publisher...\n");
    printf("Ready to echo latency measurements back to publisher\n");

    /* Wait for first sample */
    while (g_running && !g_lat_ctx.first_sample_received) {
        sleep_ms(100);
    }
    if (!g_running) goto cleanup;

    printf("First latency sample received. Starting %" PRIu64 " second timer.\n", args->execution_time);

    /* Wait for execution_time from first sample */
    uint64_t wait_end_mono = g_lat_ctx.start_mono + args->execution_time * 1000000000ULL;
    while (g_running) {
        uint64_t now_mono = get_monotonic_ns();
        if (now_mono >= wait_end_mono) break;

        uint64_t remaining_ms = (wait_end_mono - now_mono) / 1000000;
        if (remaining_ms > 1000) {
            sleep_ms(1000);
        } else if (remaining_ms > 0) {
            sleep_ms((unsigned int)remaining_ms);
            break;
        } else {
            break;
        }
    }

    /* Print results */
    printf("\n=== Latency Subscriber (Echo Mode) Results ===\n");
    printf("[INFO] Total echo responses sent: %" PRIu64 "\n", g_lat_ctx.total_samples);

    /* Log file */
    char ts_str[64]; get_log_timestamp_str(ts_str, sizeof(ts_str));
    char log_fn[256]; snprintf(log_fn, sizeof(log_fn), "sub_lat-%s.log", ts_str);
    FILE *logf = fopen(log_fn, "w");
    if (logf) {
        fprintf(logf, "Echo mode - Total responses: %" PRIu64 "\n", g_lat_ctx.total_samples);
        fclose(logf);
    }

cleanup:
    free(g_lat_ctx.recv_buf);
    g_lat_ctx.recv_buf = NULL;
    g_lat_ctx.echo_writer = NULL;
    if (reader_qos) int2dds_datareader_qos_destroy(reader_qos);
    if (writer_qos) int2dds_datawriter_qos_destroy(writer_qos);
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ================================================================
 * LOCAL LATENCY SUBSCRIBER
 * ================================================================ */

static void run_local_latency_subscriber(const subscriber_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsSubscriber *subscriber = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsDataReader *reader = NULL;
    Int2DdsDataReaderQos *qos = NULL;

    printf("Starting local latency subscriber:\n");
    printf("  Reliability: %s\n", reliability_str(args->reliability));
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    printf("\n");

    /* Init local latency context */
    memset(&g_ll_ctx, 0, sizeof(g_ll_ctx));
    g_ll_ctx.buf_capacity = 256 + args->data_len;
    g_ll_ctx.recv_buf = (uint8_t*)malloc(g_ll_ctx.buf_capacity);
    if (!g_ll_ctx.recv_buf) { fprintf(stderr, "Allocation failed\n"); return; }
    latency_collector_init(&g_ll_ctx.latency_samples, 65536);
    seqnum_set_init(&g_ll_ctx.seen_seqs, 65536);

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to get factory: %d\n", ret); goto cleanup; }

    ret = int2dds_create_participant(factory, "perftest_local_subscriber", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create participant: %d\n", ret); goto cleanup; }

    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create subscriber: %d\n", ret); goto cleanup; }

    ret = int2dds_create_topic(participant, "local_latency_test_topic", "PerformanceTestData",
                                0 /* FINAL */, NULL, &topic);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create topic: %d\n", ret); goto cleanup; }

    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) goto cleanup;
    int2dds_datareader_qos_set_reliability(qos,
        is_reliable(args->reliability) ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT);
    int2dds_datareader_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 100);

    Int2DdsDataReaderListener rd_listener = {
        .on_data_available = on_local_latency_data_available,
        .on_subscription_matched = on_local_latency_subscription_matched,
        .on_sample_rejected = NULL, .on_liveliness_changed = NULL,
        .on_requested_deadline_missed = NULL, .on_requested_incompatible_qos = NULL,
        .on_sample_lost = NULL, .user_context = NULL
    };
    ret = int2dds_create_datareader_with_listener(subscriber, topic, qos, &rd_listener,
                                                   INT2DDS_STATUS_DATA_AVAILABLE | INT2DDS_STATUS_SUBSCRIPTION_MATCHED,
                                                   &reader);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datareader: %d\n", ret); goto cleanup; }

    printf("Waiting for publisher...\n");

    /* Wait for first sample */
    while (g_running && !g_ll_ctx.first_sample_received) {
        sleep_ms(100);
    }
    if (!g_running) goto cleanup;

    /* Periodic reporting + 2-second timeout monitoring */
    uint64_t last_report_mono = g_ll_ctx.start_mono;
    uint64_t report_interval_count = 0;

    while (g_running && !g_ll_ctx.should_stop) {
        sleep_ms(100);

        /* Check 2-second timeout after last message */
        if (g_ll_ctx.last_message_mono > 0) {
            uint64_t now_mono = get_monotonic_ns();
            if (now_mono - g_ll_ctx.last_message_mono >= 2000000000ULL) {
                printf("\n2 second timeout after last message. Stopping...\n");
                g_ll_ctx.should_stop = true;
                break;
            }
        }

        /* 5-second reporting */
        report_interval_count++;
        if (report_interval_count >= 50) {
            report_interval_count = 0;

            uint32_t sample_count = g_ll_ctx.latency_samples.count;
            if (sample_count > 0) {
                double sum = 0;
                double min_ns = 1e18;
                double max_ns = 0;
                for (uint32_t i = 0; i < sample_count; i++) {
                    double v = g_ll_ctx.latency_samples.samples[i];
                    sum += v;
                    if (v < min_ns) min_ns = v;
                    if (v > max_ns) max_ns = v;
                }
                double avg_ns = sum / sample_count;
                double elapsed = (double)(get_monotonic_ns() - g_ll_ctx.start_mono) / 1e9;

                printf("[%.0fs] Samples: %u, Avg: %.3f ms, Min: %.3f ms, Max: %.3f ms\n",
                       elapsed, sample_count, avg_ns / 1e6, min_ns / 1e6, max_ns / 1e6);
            }
        }
    }

    /* Final results */
    {
        uint64_t end_mono = g_ll_ctx.last_message_mono > 0 ? g_ll_ctx.last_message_mono : get_monotonic_ns();
        double duration = (double)(end_mono - g_ll_ctx.start_mono) / 1e9;

        printf("\n=== Local Latency Test Results ===\n");
        printf("Test duration: %.2f seconds\n", duration);
        printf("[INFO] Total samples: %" PRIu64 "\n", g_ll_ctx.total_samples);

        uint32_t sample_count = g_ll_ctx.latency_samples.count;
        if (sample_count > 0) {
            double sum = 0;
            double min_ns = 1e18;
            double max_ns = 0;
            for (uint32_t i = 0; i < sample_count; i++) {
                double v = g_ll_ctx.latency_samples.samples[i];
                sum += v;
                if (v < min_ns) min_ns = v;
                if (v > max_ns) max_ns = v;
            }
            double avg_ns = sum / sample_count;

            printf("[INFO] Latency samples: %u\n", sample_count);
            printf("[INFO] Average latency: %.3f ms\n", avg_ns / 1e6);
            printf("[INFO] Min latency: %.3f ms\n", min_ns / 1e6);
            printf("[INFO] Max latency: %.3f ms\n", max_ns / 1e6);

            /* CSV */
            char csv_fn[256];
            if (args->hz > 0.0)
                snprintf(csv_fn, sizeof(csv_fn), "subscriber_local_latency_results_%zub_%.0fhz.csv", args->data_len, args->hz);
            else
                snprintf(csv_fn, sizeof(csv_fn), "subscriber_local_latency_results_%zub.csv", args->data_len);

            bool csv_exists = false;
            { FILE *test = fopen(csv_fn, "r"); if (test) { csv_exists = true; fclose(test); } }
            FILE *csv = fopen(csv_fn, "a");
            if (csv) {
                if (!csv_exists)
                    fprintf(csv, "Timestamp,Total_Samples,Avg_Latency_ms,Min_Latency_ms,Max_Latency_ms\n");
                char csv_ts[64]; get_timestamp_str(csv_ts, sizeof(csv_ts));
                fprintf(csv, "%s,%u,%.3f,%.3f,%.3f\n",
                        csv_ts, sample_count, avg_ns / 1e6, min_ns / 1e6, max_ns / 1e6);
                fclose(csv);
            }
        }

        /* Log file */
        char ts_str[64]; get_log_timestamp_str(ts_str, sizeof(ts_str));
        char log_fn[256]; snprintf(log_fn, sizeof(log_fn), "sub_local_lat-%s.log", ts_str);
        FILE *logf = fopen(log_fn, "w");
        if (logf) {
            fprintf(logf, "Local Latency Test\nTotal samples: %" PRIu64 "\n", g_ll_ctx.total_samples);
            fclose(logf);
        }
    }

cleanup:
    latency_collector_destroy(&g_ll_ctx.latency_samples);
    seqnum_set_destroy(&g_ll_ctx.seen_seqs);
    free(g_ll_ctx.recv_buf);
    g_ll_ctx.recv_buf = NULL;
    if (qos) int2dds_datareader_qos_destroy(qos);
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ================================================================
 * MAIN
 * ================================================================ */

int main(int argc, char *argv[]) {
    subscriber_args_t args;

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    parse_args(argc, argv, &args);

    printf("DDS Performance Test Subscriber\n");
    printf("Test mode: %s\n", args.test_mode);

    if (strcmp(args.test_mode, "throughput") == 0 ||
        strcmp(args.test_mode, "thr") == 0 ||
        strcmp(args.test_mode, "1") == 0) {
        printf("Starting throughput test mode\n");
        run_throughput_subscriber(&args);
    } else if (strcmp(args.test_mode, "latency") == 0 ||
               strcmp(args.test_mode, "lat") == 0 ||
               strcmp(args.test_mode, "2") == 0) {
        printf("Starting latency test mode\n");
        run_latency_subscriber(&args);
    } else if (strcmp(args.test_mode, "local_latency") == 0 ||
               strcmp(args.test_mode, "local") == 0 ||
               strcmp(args.test_mode, "ll") == 0 ||
               strcmp(args.test_mode, "3") == 0) {
        printf("Starting local latency test mode\n");
        run_local_latency_subscriber(&args);
    } else {
        printf("Invalid test mode '%s'. Supported modes: throughput, latency, local_latency\n", args.test_mode);
        printf("Defaulting to throughput test.\n");
        run_throughput_subscriber(&args);
    }

    return 0;
}
