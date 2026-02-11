/*
 * int2dds FFI Performance Test Publisher
 *
 * Uses IDL-generated CDR serialization (LatencyTestData.h, PerformanceTestData.h)
 * with int2dds_write_serialized / int2dds_take_serialized API.
 *
 * Three test modes:
 *   - throughput: Measures message throughput and bandwidth
 *   - latency:    Measures round-trip latency with echo from subscriber
 *   - local_latency: Measures one-way latency (subscriber measures)
 *
 * Arguments match the Rust perftest_publisher exactly.
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

static void precise_sleep_ns(uint64_t ns) {
    if (ns > 1000000) {
        Sleep((DWORD)(ns / 1000000));
    }
}

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

static void precise_sleep_ns(uint64_t ns) {
    struct timespec ts;
    ts.tv_sec = (time_t)(ns / 1000000000ULL);
    ts.tv_nsec = (long)(ns % 1000000000ULL);
    nanosleep(&ts, NULL);
}

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
 * PerformanceTestData (APPENDABLE):
 *   [0..3]   encapsulation header
 *   [4..7]   DHEADER (struct size)
 *   [8..15]  seq_num (u64)
 *   [16..23] timestamp (u64)
 *   [24..27] data.length (u32)
 *   [28..]   data bytes
 *
 * LatencyTestData (APPENDABLE):
 *   [0..3]   encapsulation header
 *   [4..7]   DHEADER (struct size)
 *   [8..15]  seq_num (u64)
 *   [16..23] send_timestamp (u64)
 *   [24..31] echo_timestamp (u64)
 *   [32..35] data.length (u32)
 *   [36..]   data bytes
 */
#define PERFDATA_CDR_SEQNUM_OFFSET     8
#define PERFDATA_CDR_TIMESTAMP_OFFSET  16

#define LATDATA_CDR_SEQNUM_OFFSET      8
#define LATDATA_CDR_SEND_TS_OFFSET     16
#define LATDATA_CDR_ECHO_TS_OFFSET     24

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

/* ====== Latency Stats (updated by echo callback) ====== */

typedef struct {
    uint64_t latency_count;
    double   sum_latency_ns;
    double   min_latency_ns;
    double   max_latency_ns;
    uint64_t total_samples;
} latency_stats_t;

/* ====== Echo Receive Context ====== */

typedef struct {
    uint8_t*        recv_buf;
    size_t          buf_capacity;
    latency_stats_t stats;
} echo_recv_ctx_t;

static echo_recv_ctx_t g_echo_ctx;

/* ====== Listener: Echo data available (publisher receives echo responses) ====== */

static void on_echo_data_available(Int2DdsDataReader* reader, Int2DdsUserContext ctx) {
    (void)ctx;
    uint64_t current_time = get_current_time_ns();

    uintptr_t actual_size;
    bool valid_data;

    while (1) {
        Int2DdsRet ret = int2dds_take_serialized(reader, g_echo_ctx.recv_buf,
                                                  g_echo_ctx.buf_capacity,
                                                  &actual_size, &valid_data);
        if (ret == INT2DDS_RET_NO_DATA) break;
        if (ret != INT2DDS_RET_OK || !valid_data) continue;

        /* Read send_timestamp directly from CDR offset */
        uint64_t send_ts;
        memcpy(&send_ts, g_echo_ctx.recv_buf + LATDATA_CDR_SEND_TS_OFFSET, sizeof(uint64_t));

        /* RTT = current_time - send_timestamp; one-way = RTT / 2 */
        uint64_t rtt_ns = current_time - send_ts;
        double one_way_ns = (double)rtt_ns / 2.0;

        g_echo_ctx.stats.latency_count++;
        g_echo_ctx.stats.sum_latency_ns += one_way_ns;
        g_echo_ctx.stats.total_samples++;

        if (g_echo_ctx.stats.latency_count == 1) {
            g_echo_ctx.stats.min_latency_ns = one_way_ns;
            g_echo_ctx.stats.max_latency_ns = one_way_ns;
        } else {
            if (one_way_ns < g_echo_ctx.stats.min_latency_ns)
                g_echo_ctx.stats.min_latency_ns = one_way_ns;
            if (one_way_ns > g_echo_ctx.stats.max_latency_ns)
                g_echo_ctx.stats.max_latency_ns = one_way_ns;
        }
    }
}

/* ====== Listener: Publication matched ====== */

static void on_publication_matched(
    Int2DdsDataWriter* writer,
    const Int2DdsPublicationMatchedStatus* status,
    Int2DdsUserContext ctx)
{
    (void)writer; (void)ctx;
    if (status->current_count > 0) {
        printf("Subscriber matched! (total: %d, current: %d)\n",
               status->total_count, status->current_count);
    } else {
        printf("No subscribers matched.\n");
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
    size_t   batch_size;
    uint64_t latency_count;
} publisher_args_t;

static void print_usage(const char *prog) {
    printf("Usage: %s [OPTIONS]\n", prog);
    printf("  --test-mode MODE       throughput|latency|local_latency (default: throughput)\n");
    printf("  --domain-id ID         Domain ID (default: 10)\n");
    printf("  --data-len SIZE        Data size in bytes (default: 1024)\n");
    printf("  --execution-time SEC   Test duration in seconds (default: 30)\n");
    printf("  --reliability MODE     best|besteffort|best_effort|reliable (default: best)\n");
    printf("  --hz RATE              Rate limit in Hz (optional)\n");
    printf("  --warmup-time SEC      Warmup sleep in seconds (default: 1)\n");
    printf("  --batch-size SIZE      Batch size (default: 8192)\n");
    printf("  --latency-count N      Number of latency samples (optional)\n");
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

static void parse_args(int argc, char *argv[], publisher_args_t *args) {
    strcpy(args->test_mode, "throughput");
    args->domain_id = 10;
    args->data_len = 1024;
    args->execution_time = 30;
    strcpy(args->reliability, "best");
    args->hz = 0.0;
    args->warmup_time = 1;
    args->batch_size = 8192;
    args->latency_count = 0;

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
        } else if ((val = match_arg(argc, argv, &i, "--batch-size")) != NULL) {
            args->batch_size = (size_t)strtoull(val, NULL, 10);
        } else if ((val = match_arg(argc, argv, &i, "--latency-count")) != NULL) {
            args->latency_count = (uint64_t)strtoull(val, NULL, 10);
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

/* ====== CSV timestamp helper ====== */

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

/* ====== DDS Setup Helpers ====== */

static Int2DdsRet setup_factory_and_participant(
    const char *name, int32_t domain_id,
    Int2DdsParticipantFactory **factory_out,
    Int2DdsParticipant **participant_out)
{
    Int2DdsRet ret = int2dds_domain_participant_factory_get_instance(factory_out);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return ret;
    }
    ret = int2dds_create_participant(*factory_out, name, domain_id, participant_out);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
    }
    return ret;
}

static Int2DdsRet wait_for_subscriber(Int2DdsDataWriter *writer) {
    Int2DdsWaitSet *waitset = NULL;
    Int2DdsRet ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) return ret;

    ret = int2dds_waitset_attach_datawriter(waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        int2dds_waitset_delete(waitset);
        return ret;
    }

    ret = int2dds_waitset_wait(waitset, -1);
    int2dds_waitset_detach_datawriter(waitset, writer);
    int2dds_waitset_delete(waitset);
    return ret;
}

/* ====== Hz-based rate limiter ====== */

typedef struct {
    bool     enabled;
    uint64_t interval_ns;
    uint64_t next_send_mono;
} rate_limiter_t;

static void rate_limiter_init(rate_limiter_t *rl, double hz) {
    if (hz > 0.0) {
        rl->enabled = true;
        rl->interval_ns = (uint64_t)(1000000000.0 / hz);
        rl->next_send_mono = get_monotonic_ns();
    } else {
        rl->enabled = false;
        rl->interval_ns = 0;
        rl->next_send_mono = 0;
    }
}

static void rate_limiter_wait(rate_limiter_t *rl) {
    if (!rl->enabled) return;
    rl->next_send_mono += rl->interval_ns;
    uint64_t now = get_monotonic_ns();
    if (rl->next_send_mono > now) {
        precise_sleep_ns(rl->next_send_mono - now);
    }
}

/* ================================================================
 * THROUGHPUT TEST
 * ================================================================ */

static void run_throughput_test(const publisher_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsPublisher *publisher = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsDataWriter *writer = NULL;
    Int2DdsDataWriterQos *qos = NULL;

    printf("Starting throughput test:\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", reliability_str(args->reliability));
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    printf("  Batch size: %zu\n", args->batch_size);
    if (args->hz > 0.0) printf("  Rate: %.1f Hz\n", args->hz);
    printf("\n");

    ret = setup_factory_and_participant("perftest_publisher", args->domain_id, &factory, &participant);
    if (ret != INT2DDS_RET_OK) goto cleanup;

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create publisher: %d\n", ret); goto cleanup; }

    ret = int2dds_create_topic(participant, "throughput_test_topic", "ThroughputTestData",
                                1 /* APPENDABLE */, NULL, &topic);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create topic: %d\n", ret); goto cleanup; }

    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) goto cleanup;

    int2dds_datawriter_qos_set_reliability(qos,
        is_reliable(args->reliability) ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        100000000);
    int2dds_datawriter_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);

    Int2DdsDataWriterListener wr_listener = {
        .on_publication_matched = on_publication_matched,
        .on_offered_deadline_missed = NULL, .on_offered_incompatible_qos = NULL,
        .on_liveliness_lost = NULL, .user_context = NULL
    };
    ret = int2dds_create_datawriter_with_listener(publisher, topic, qos, &wr_listener,
                                                   INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datawriter: %d\n", ret); goto cleanup; }

    printf("Waiting for subscriber to connect...\n");
    ret = wait_for_subscriber(writer);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "WaitSet wait failed: %d\n", ret); goto cleanup; }
    printf("Subscriber connected. Starting throughput test...\n");

    /* Warmup */
    if (args->warmup_time > 0) {
        printf("Warmup phase: sleeping for %" PRIu64 " seconds...\n", args->warmup_time);
        sleep_ms((unsigned int)(args->warmup_time * 1000));
        printf("Warmup complete. Starting actual measurement...\n");
    }

    /* Pre-serialize template buffer */
    size_t buf_capacity = 256 + args->data_len;
    uint8_t *send_buf = (uint8_t*)malloc(buf_capacity);
    uint8_t *payload = (uint8_t*)calloc(args->data_len, 1);
    if (!send_buf || !payload) { fprintf(stderr, "Allocation failed\n"); goto cleanup; }

    PerformanceTestData tmpl = {
        .seq_num = 0, .timestamp = 0,
        .data = { .data = payload, .length = (uint32_t)args->data_len }
    };
    size_t template_len = PerformanceTestData_serialize_cdr(&tmpl, send_buf, buf_capacity);
    if (template_len == 0) { fprintf(stderr, "Serialization failed\n"); goto cleanup; }

    /* Send loop */
    uint64_t start_mono = get_monotonic_ns();
    uint64_t end_mono = start_mono + args->execution_time * 1000000000ULL;
    uint64_t seq_num = 0;
    uint64_t total_sent = 0;
    uint64_t last_report_mono = start_mono;
    uint64_t last_report_count = 0;

    rate_limiter_t rl;
    rate_limiter_init(&rl, args->hz);

    while (g_running && get_monotonic_ns() < end_mono) {
        /* Update seq_num and timestamp in-place in CDR buffer */
        memcpy(send_buf + PERFDATA_CDR_SEQNUM_OFFSET, &seq_num, sizeof(uint64_t));
        uint64_t ts = get_current_time_ns();
        memcpy(send_buf + PERFDATA_CDR_TIMESTAMP_OFFSET, &ts, sizeof(uint64_t));

        ret = int2dds_write_serialized(writer, send_buf, template_len, NULL, 0);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Write failed: %d\n", ret);
            break;
        }

        seq_num++;
        total_sent++;

        rate_limiter_wait(&rl);

        /* 5-second reporting */
        uint64_t now_mono = get_monotonic_ns();
        double report_elapsed = (double)(now_mono - last_report_mono) / 1e9;
        if (report_elapsed >= 5.0) {
            double elapsed = (double)(now_mono - start_mono) / 1e9;
            uint64_t interval_sent = total_sent - last_report_count;

            double avg_rate = total_sent / elapsed;
            double interval_rate = interval_sent / report_elapsed;
            double avg_mbps = (total_sent * args->data_len * 8.0) / (elapsed * 1e6);
            double interval_mbps = (interval_sent * args->data_len * 8.0) / (report_elapsed * 1e6);

            printf("[%.1fs] Sent: %" PRIu64 ", Avg: %.0f msg/s (%.2f Mbps), "
                   "Interval: %.0f msg/s (%.2f Mbps)\n",
                   elapsed, total_sent, avg_rate, avg_mbps, interval_rate, interval_mbps);

            last_report_mono = now_mono;
            last_report_count = total_sent;
        }
    }

    /* Final results */
    double elapsed = (double)(get_monotonic_ns() - start_mono) / 1e9;
    double final_rate = total_sent / elapsed;
    double mbps = (total_sent * args->data_len * 8.0) / (elapsed * 1e6);

    printf("\n=== Throughput Test Publisher Results ===\n");
    printf("[INFO] Total samples sent: %" PRIu64 "\n", total_sent);
    printf("Test duration: %.2f seconds\n", elapsed);
    printf("[INFO] Send rate: %.2f msg/s\n", final_rate);
    printf("Throughput: %.2f Mbps\n", mbps);

    /* Log file */
    char ts_str[64];
    get_log_timestamp_str(ts_str, sizeof(ts_str));
    char log_fn[256];
    snprintf(log_fn, sizeof(log_fn), "pub_thr-%s.log", ts_str);
    FILE *logf = fopen(log_fn, "w");
    if (logf) {
        fprintf(logf, "Total sent: %" PRIu64 "\nDuration: %.2fs\nRate: %.2f msg/s\nThroughput: %.2f Mbps\n",
                total_sent, elapsed, final_rate, mbps);
        fclose(logf);
    }

    /* CSV */
    char csv_fn[256];
    if (args->hz > 0.0)
        snprintf(csv_fn, sizeof(csv_fn), "publisher_throughput_results_%zub_%.0fhz.csv", args->data_len, args->hz);
    else
        snprintf(csv_fn, sizeof(csv_fn), "publisher_throughput_results_%zub.csv", args->data_len);

    bool csv_exists = false;
    { FILE *test = fopen(csv_fn, "r"); if (test) { csv_exists = true; fclose(test); } }
    FILE *csv = fopen(csv_fn, "a");
    if (csv) {
        if (!csv_exists)
            fprintf(csv, "Timestamp,Total_Sent_Messages,Duration_Seconds,Messages_Per_Second,Throughput_Mbps\n");
        char csv_ts[64]; get_timestamp_str(csv_ts, sizeof(csv_ts));
        fprintf(csv, "%s,%" PRIu64 ",%.2f,%.2f,%.2f\n", csv_ts, total_sent, elapsed, final_rate, mbps);
        fclose(csv);
    }

    free(send_buf);
    free(payload);

cleanup:
    if (qos) int2dds_datawriter_qos_destroy(qos);
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ================================================================
 * LATENCY TEST
 * ================================================================ */

static void run_latency_test(const publisher_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsPublisher *publisher = NULL;
    Int2DdsSubscriber *subscriber = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsTopic *echo_topic = NULL;
    Int2DdsDataWriter *writer = NULL;
    Int2DdsDataReader *reader = NULL;
    Int2DdsDataWriterQos *writer_qos = NULL;
    Int2DdsDataReaderQos *reader_qos = NULL;

    printf("Starting latency test:\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", reliability_str(args->reliability));
    if (args->latency_count > 0)
        printf("  Latency count: %" PRIu64 " samples\n", args->latency_count);
    else
        printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    if (args->hz > 0.0)
        printf("  Rate: %.1f Hz (%.3f ms interval)\n", args->hz, 1000.0 / args->hz);
    else
        printf("  Rate: unlimited\n");
    printf("\n");

    /* Reset echo stats */
    memset(&g_echo_ctx, 0, sizeof(g_echo_ctx));

    ret = setup_factory_and_participant("perftest_latency_publisher", args->domain_id, &factory, &participant);
    if (ret != INT2DDS_RET_OK) goto cleanup;

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create publisher: %d\n", ret); goto cleanup; }

    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create subscriber: %d\n", ret); goto cleanup; }

    ret = int2dds_create_topic(participant, "latency_test_topic", "LatencyTestData",
                                1 /* APPENDABLE */, NULL, &topic);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create topic: %d\n", ret); goto cleanup; }

    ret = int2dds_create_topic(participant, "latency_test_topic_echo", "LatencyTestData",
                                1 /* APPENDABLE */, NULL, &echo_topic);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create echo topic: %d\n", ret); goto cleanup; }

    /* Writer QoS */
    ret = int2dds_datawriter_qos_create_default(&writer_qos);
    if (ret != INT2DDS_RET_OK) goto cleanup;
    int2dds_datawriter_qos_set_reliability(writer_qos,
        is_reliable(args->reliability) ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        100000000);
    int2dds_datawriter_qos_set_history(writer_qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);

    /* Reader QoS */
    ret = int2dds_datareader_qos_create_default(&reader_qos);
    if (ret != INT2DDS_RET_OK) goto cleanup;
    int2dds_datareader_qos_set_reliability(reader_qos,
        is_reliable(args->reliability) ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT);

    /* Writer with listener */
    Int2DdsDataWriterListener wr_listener = {
        .on_publication_matched = on_publication_matched,
        .on_offered_deadline_missed = NULL, .on_offered_incompatible_qos = NULL,
        .on_liveliness_lost = NULL, .user_context = NULL
    };
    ret = int2dds_create_datawriter_with_listener(publisher, topic, writer_qos, &wr_listener,
                                                   INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datawriter: %d\n", ret); goto cleanup; }

    /* Allocate echo receive buffer */
    g_echo_ctx.buf_capacity = 256 + args->data_len;
    g_echo_ctx.recv_buf = (uint8_t*)malloc(g_echo_ctx.buf_capacity);
    if (!g_echo_ctx.recv_buf) { fprintf(stderr, "Allocation failed\n"); goto cleanup; }

    /* Reader with listener for echoes */
    Int2DdsDataReaderListener rd_listener = {
        .on_data_available = on_echo_data_available,
        .on_subscription_matched = NULL,
        .on_sample_rejected = NULL, .on_liveliness_changed = NULL,
        .on_requested_deadline_missed = NULL, .on_requested_incompatible_qos = NULL,
        .on_sample_lost = NULL, .user_context = NULL
    };
    ret = int2dds_create_datareader_with_listener(subscriber, echo_topic, reader_qos, &rd_listener,
                                                   INT2DDS_STATUS_DATA_AVAILABLE, &reader);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datareader: %d\n", ret); goto cleanup; }

    printf("Waiting for subscriber to connect...\n");
    ret = wait_for_subscriber(writer);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "WaitSet wait failed: %d\n", ret); goto cleanup; }
    printf("Subscriber connected. Starting latency test...\n");

    /* Warmup */
    if (args->warmup_time > 0) {
        printf("Warmup phase: sleeping for %" PRIu64 " seconds...\n", args->warmup_time);
        sleep_ms((unsigned int)(args->warmup_time * 1000));
        printf("Warmup complete. Starting actual measurement...\n");
    }

    /* Pre-serialize template */
    size_t buf_capacity = 256 + args->data_len;
    uint8_t *send_buf = (uint8_t*)malloc(buf_capacity);
    uint8_t *payload = (uint8_t*)calloc(args->data_len, 1);
    if (!send_buf || !payload) { fprintf(stderr, "Allocation failed\n"); goto cleanup; }

    LatencyTestData tmpl = {
        .seq_num = 0, .send_timestamp = 0, .echo_timestamp = 0,
        .data = { .data = payload, .length = (uint32_t)args->data_len }
    };
    size_t template_len = LatencyTestData_serialize_cdr(&tmpl, send_buf, buf_capacity);
    if (template_len == 0) { fprintf(stderr, "Serialization failed\n"); goto cleanup; }

    /* Send loop */
    uint64_t start_mono = get_monotonic_ns();
    uint64_t end_mono = start_mono + args->execution_time * 1000000000ULL;
    uint64_t seq_num = 0;

    rate_limiter_t rl;
    rate_limiter_init(&rl, args->hz);

    if (args->latency_count > 0) {
        /* Count-based mode */
        printf("Sending %" PRIu64 " latency samples (max %" PRIu64 " seconds)...\n",
               args->latency_count, args->execution_time);

        while (seq_num < args->latency_count && g_running && get_monotonic_ns() < end_mono) {
            memcpy(send_buf + LATDATA_CDR_SEQNUM_OFFSET, &seq_num, sizeof(uint64_t));
            uint64_t ts = get_current_time_ns();
            memcpy(send_buf + LATDATA_CDR_SEND_TS_OFFSET, &ts, sizeof(uint64_t));
            uint64_t zero = 0;
            memcpy(send_buf + LATDATA_CDR_ECHO_TS_OFFSET, &zero, sizeof(uint64_t));

            ret = int2dds_write_serialized(writer, send_buf, template_len, NULL, 0);
            if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Write failed: %d\n", ret); break; }

            seq_num++;

            if (seq_num % 100 == 0) {
                printf("Sent: %" PRIu64 " / %" PRIu64 "\n", seq_num, args->latency_count);
            }

            rate_limiter_wait(&rl);
        }

        if (seq_num < args->latency_count) {
            printf("Test stopped by execution_time limit. Sent %" PRIu64 " / %" PRIu64 " samples.\n",
                   seq_num, args->latency_count);
        }
    } else {
        /* Time-based mode */
        while (g_running && get_monotonic_ns() < end_mono) {
            memcpy(send_buf + LATDATA_CDR_SEQNUM_OFFSET, &seq_num, sizeof(uint64_t));
            uint64_t ts = get_current_time_ns();
            memcpy(send_buf + LATDATA_CDR_SEND_TS_OFFSET, &ts, sizeof(uint64_t));
            uint64_t zero = 0;
            memcpy(send_buf + LATDATA_CDR_ECHO_TS_OFFSET, &zero, sizeof(uint64_t));

            ret = int2dds_write_serialized(writer, send_buf, template_len, NULL, 0);
            if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Write failed: %d\n", ret); break; }

            seq_num++;
            rate_limiter_wait(&rl);
        }
    }

    /* Wait for final echoes */
    sleep_ms(1000);

    /* Print results */
    printf("\n=== Performance Test Results ===\n");
    double elapsed = (double)(get_monotonic_ns() - start_mono) / 1e9;
    printf("Test duration: %.2f seconds\n", elapsed);
    printf("[INFO] Total samples received: %" PRIu64 "\n", g_echo_ctx.stats.total_samples);

    if (g_echo_ctx.stats.latency_count > 0) {
        double avg = g_echo_ctx.stats.sum_latency_ns / g_echo_ctx.stats.latency_count;
        double min_lat = g_echo_ctx.stats.min_latency_ns;
        double max_lat = g_echo_ctx.stats.max_latency_ns;

        printf("[INFO] Latency samples: %" PRIu64 "\n", g_echo_ctx.stats.latency_count);
        printf("[INFO] Average latency: %.3f ms (%.2f us)\n", avg / 1e6, avg / 1e3);
        printf("[INFO] Min latency: %.3f ms (%.2f us)\n", min_lat / 1e6, min_lat / 1e3);
        printf("[INFO] Max latency: %.3f ms (%.2f us)\n", max_lat / 1e6, max_lat / 1e3);

        /* Log file */
        char ts_str[64]; get_log_timestamp_str(ts_str, sizeof(ts_str));
        char log_fn[256]; snprintf(log_fn, sizeof(log_fn), "pub_lat-%s.log", ts_str);
        FILE *logf = fopen(log_fn, "w");
        if (logf) {
            fprintf(logf, "Latency samples: %" PRIu64 "\nAvg latency: %.3f ms (%.2f us)\n"
                    "Min latency: %.3f ms (%.2f us)\nMax latency: %.3f ms (%.2f us)\n",
                    g_echo_ctx.stats.latency_count,
                    avg / 1e6, avg / 1e3, min_lat / 1e6, min_lat / 1e3, max_lat / 1e6, max_lat / 1e3);
            fclose(logf);
        }

        /* CSV */
        char csv_fn[256];
        if (args->hz > 0.0)
            snprintf(csv_fn, sizeof(csv_fn), "publisher_latency_results_%zub_%.0fhz.csv", args->data_len, args->hz);
        else
            snprintf(csv_fn, sizeof(csv_fn), "publisher_latency_results_%zub_unlimited.csv", args->data_len);

        bool csv_exists = false;
        { FILE *test = fopen(csv_fn, "r"); if (test) { csv_exists = true; fclose(test); } }
        FILE *csv = fopen(csv_fn, "a");
        if (csv) {
            if (!csv_exists) fprintf(csv, "Timestamp,Average_Latency_ms,Min_Latency_ms,Max_Latency_ms\n");
            char csv_ts[64]; get_timestamp_str(csv_ts, sizeof(csv_ts));
            fprintf(csv, "%s,%.3f,%.3f,%.3f\n", csv_ts, avg / 1e6, min_lat / 1e6, max_lat / 1e6);
            fclose(csv);
        }
    } else {
        printf("No latency samples received\n");
    }

    free(send_buf);
    free(payload);

cleanup:
    if (g_echo_ctx.recv_buf) { free(g_echo_ctx.recv_buf); g_echo_ctx.recv_buf = NULL; }
    if (writer_qos) int2dds_datawriter_qos_destroy(writer_qos);
    if (reader_qos) int2dds_datareader_qos_destroy(reader_qos);
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ================================================================
 * LOCAL LATENCY TEST
 * ================================================================ */

static void run_local_latency_test(const publisher_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsPublisher *publisher = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsDataWriter *writer = NULL;
    Int2DdsDataWriterQos *qos = NULL;

    printf("Starting local latency test (publisher):\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", reliability_str(args->reliability));
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    if (args->hz > 0.0)
        printf("  Rate: %.1f Hz (%.3f ms interval)\n", args->hz, 1000.0 / args->hz);
    else
        printf("  Rate: unlimited\n");
    printf("\n");

    ret = setup_factory_and_participant("perftest_local_publisher", args->domain_id, &factory, &participant);
    if (ret != INT2DDS_RET_OK) goto cleanup;

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create publisher: %d\n", ret); goto cleanup; }

    ret = int2dds_create_topic(participant, "local_latency_test_topic", "ThroughputTestData",
                                1 /* APPENDABLE */, NULL, &topic);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create topic: %d\n", ret); goto cleanup; }

    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) goto cleanup;
    int2dds_datawriter_qos_set_reliability(qos,
        is_reliable(args->reliability) ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        100000000);
    int2dds_datawriter_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);

    Int2DdsDataWriterListener wr_listener = {
        .on_publication_matched = on_publication_matched,
        .on_offered_deadline_missed = NULL, .on_offered_incompatible_qos = NULL,
        .on_liveliness_lost = NULL, .user_context = NULL
    };
    ret = int2dds_create_datawriter_with_listener(publisher, topic, qos, &wr_listener,
                                                   INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datawriter: %d\n", ret); goto cleanup; }

    printf("Waiting for subscriber to connect...\n");
    ret = wait_for_subscriber(writer);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "WaitSet wait failed: %d\n", ret); goto cleanup; }
    printf("Subscriber connected. Starting local latency test...\n");

    /* Warmup */
    if (args->warmup_time > 0) {
        printf("Warmup phase: sleeping for %" PRIu64 " seconds...\n", args->warmup_time);
        sleep_ms((unsigned int)(args->warmup_time * 1000));
        printf("Warmup complete. Starting actual measurement...\n");
    }

    /* Pre-serialize template */
    size_t buf_capacity = 256 + args->data_len;
    uint8_t *send_buf = (uint8_t*)malloc(buf_capacity);
    uint8_t *payload = (uint8_t*)calloc(args->data_len, 1);
    if (!send_buf || !payload) { fprintf(stderr, "Allocation failed\n"); goto cleanup; }

    PerformanceTestData tmpl = {
        .seq_num = 0, .timestamp = 0,
        .data = { .data = payload, .length = (uint32_t)args->data_len }
    };
    size_t template_len = PerformanceTestData_serialize_cdr(&tmpl, send_buf, buf_capacity);
    if (template_len == 0) { fprintf(stderr, "Serialization failed\n"); goto cleanup; }

    /* Send loop */
    uint64_t start_mono = get_monotonic_ns();
    uint64_t end_mono = start_mono + args->execution_time * 1000000000ULL;
    uint64_t seq_num = 0;
    uint64_t total_sent = 0;
    uint64_t last_report_mono = start_mono;
    uint64_t last_report_count = 0;

    rate_limiter_t rl;
    rate_limiter_init(&rl, args->hz);

    while (g_running && get_monotonic_ns() < end_mono) {
        memcpy(send_buf + PERFDATA_CDR_SEQNUM_OFFSET, &seq_num, sizeof(uint64_t));
        uint64_t ts = get_current_time_ns();
        memcpy(send_buf + PERFDATA_CDR_TIMESTAMP_OFFSET, &ts, sizeof(uint64_t));

        ret = int2dds_write_serialized(writer, send_buf, template_len, NULL, 0);
        if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Write failed: %d\n", ret); break; }

        seq_num++;
        total_sent++;

        rate_limiter_wait(&rl);

        /* 5-second reporting */
        uint64_t now_mono = get_monotonic_ns();
        double report_elapsed = (double)(now_mono - last_report_mono) / 1e9;
        if (report_elapsed >= 5.0) {
            double elapsed = (double)(now_mono - start_mono) / 1e9;
            uint64_t interval_sent = total_sent - last_report_count;
            double avg_rate = total_sent / elapsed;
            double interval_rate = interval_sent / report_elapsed;

            printf("[%.1fs] Sent: %" PRIu64 ", Avg: %.0f msg/s, Interval: %.0f msg/s\n",
                   elapsed, total_sent, avg_rate, interval_rate);

            last_report_mono = now_mono;
            last_report_count = total_sent;
        }
    }

    /* Final results */
    double elapsed = (double)(get_monotonic_ns() - start_mono) / 1e9;
    double final_rate = total_sent / elapsed;

    printf("\n=== Local Latency Test Publisher Results ===\n");
    printf("[INFO] Total samples sent: %" PRIu64 "\n", total_sent);
    printf("Test duration: %.2f seconds\n", elapsed);
    printf("[INFO] Send rate: %.2f msg/s\n", final_rate);

    /* Log file */
    char ts_str[64]; get_log_timestamp_str(ts_str, sizeof(ts_str));
    char log_fn[256]; snprintf(log_fn, sizeof(log_fn), "pub_local_lat-%s.log", ts_str);
    FILE *logf = fopen(log_fn, "w");
    if (logf) {
        fprintf(logf, "Local Latency Test Publisher\nTotal sent: %" PRIu64 "\nDuration: %.2fs\nRate: %.2f msg/s\n",
                total_sent, elapsed, final_rate);
        fclose(logf);
    }

    /* CSV */
    char csv_fn[256];
    if (args->hz > 0.0)
        snprintf(csv_fn, sizeof(csv_fn), "publisher_local_latency_results_%zub_%.0fhz.csv", args->data_len, args->hz);
    else
        snprintf(csv_fn, sizeof(csv_fn), "publisher_local_latency_results_%zub.csv", args->data_len);

    bool csv_exists = false;
    { FILE *test = fopen(csv_fn, "r"); if (test) { csv_exists = true; fclose(test); } }
    FILE *csv = fopen(csv_fn, "a");
    if (csv) {
        if (!csv_exists)
            fprintf(csv, "Timestamp,Total_Sent_Messages,Duration_Seconds,Messages_Per_Second\n");
        char csv_ts[64]; get_timestamp_str(csv_ts, sizeof(csv_ts));
        fprintf(csv, "%s,%" PRIu64 ",%.2f,%.2f\n", csv_ts, total_sent, elapsed, final_rate);
        fclose(csv);
    }

    free(send_buf);
    free(payload);

cleanup:
    if (qos) int2dds_datawriter_qos_destroy(qos);
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
    publisher_args_t args;

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    parse_args(argc, argv, &args);

    printf("DDS Performance Test Publisher\n");
    printf("Test mode: %s\n", args.test_mode);

    if (strcmp(args.test_mode, "throughput") == 0 ||
        strcmp(args.test_mode, "thr") == 0 ||
        strcmp(args.test_mode, "1") == 0) {
        printf("Starting throughput test mode\n");
        run_throughput_test(&args);
    } else if (strcmp(args.test_mode, "latency") == 0 ||
               strcmp(args.test_mode, "lat") == 0 ||
               strcmp(args.test_mode, "2") == 0) {
        printf("Starting latency test mode\n");
        run_latency_test(&args);
    } else if (strcmp(args.test_mode, "local_latency") == 0 ||
               strcmp(args.test_mode, "local") == 0 ||
               strcmp(args.test_mode, "ll") == 0 ||
               strcmp(args.test_mode, "3") == 0) {
        printf("Starting local latency test mode\n");
        run_local_latency_test(&args);
    } else {
        printf("Invalid test mode '%s'. Supported modes: throughput, latency, local_latency\n", args.test_mode);
        printf("Defaulting to throughput test.\n");
        run_throughput_test(&args);
    }

    return 0;
}
