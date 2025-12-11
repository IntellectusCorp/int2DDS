/*
 * int2dds FFI Performance Test Publisher
 *
 * Based on CycloneDDS performance test, converted to use int2dds FFI API.
 * This program measures DDS performance in different modes:
 * - Throughput: Measures message throughput and bandwidth
 * - Latency: Measures round-trip latency with echo from subscriber
 * - Local Latency: Measures local loopback latency (one-way)
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <signal.h>
#include <stdint.h>
#include <stdbool.h>
#include <inttypes.h>

#ifdef _WIN32
#include <windows.h>
#include <process.h>
#define usleep(x) Sleep((x) / 1000)
#define sleep(x) Sleep((x) * 1000)
#define sleep_ms(ms) Sleep(ms)

static int nanosleep_impl(const struct timespec *req, struct timespec *rem) {
    (void)rem;
    Sleep((DWORD)(req->tv_sec * 1000 + req->tv_nsec / 1000000));
    return 0;
}
#define nanosleep(req, rem) nanosleep_impl(req, rem)

#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1

/* Windows implementation of clock_gettime */
static int clock_gettime(int clock_id, struct timespec *ts) {
    (void)clock_id;
    FILETIME ft;
    ULARGE_INTEGER uli;
    GetSystemTimeAsFileTime(&ft);
    uli.LowPart = ft.dwLowDateTime;
    uli.HighPart = ft.dwHighDateTime;

    /* Convert from 100-nanosecond intervals since 1601-01-01 to seconds since 1970-01-01 */
    uli.QuadPart -= 116444736000000000ULL;
    ts->tv_sec = (time_t)(uli.QuadPart / 10000000ULL);
    ts->tv_nsec = (long)((uli.QuadPart % 10000000ULL) * 100);

    return 0;
}
#else
#include <unistd.h>
#include <getopt.h>
#define sleep_ms(ms) usleep((ms) * 1000)
#endif

#include "int2dds-ffi.h"

#define MAX_SAMPLES 100

/* ====== Data Structures ====== */

/* Performance test data structure */
typedef struct {
    uint64_t seq_num;
    uint64_t timestamp;
    uint8_t* data;
    size_t data_len;
} PerformanceTestData;

/* Latency test data structure */
typedef struct {
    uint64_t seq_num;
    uint64_t send_timestamp;
    uint64_t echo_timestamp;
    uint8_t* data;
    size_t data_len;
} LatencyTestData;

/* Performance statistics */
typedef struct {
    uint64_t total_sent;
    uint64_t total_bytes;
    uint64_t lost_samples;
    uint64_t latency_samples;
    double min_latency;
    double max_latency;
    double sum_latency;
    struct timespec start_time;
    struct timespec end_time;
} performance_stats_t;

/* Command-line arguments */
typedef struct {
    char test_mode[32];
    int32_t domain_id;
    size_t data_len;
    uint64_t execution_time;
    char reliability[32];
    double hz;
    size_t batch_size;
    uint64_t latency_count;
    uint64_t latency_interval_ms;
} publisher_args_t;

/* Global state */
static volatile bool running = true;
static performance_stats_t g_latency_stats = {0};
static uint8_t* g_latency_buffer = NULL;
static size_t g_latency_buffer_size = 0;

/* ====== Signal Handlers ====== */

static void signal_handler(int sig) {
    (void)sig;
    running = false;
}

/* ====== Timing Functions ====== */

static uint64_t get_current_time_ns(void) {
#ifdef _WIN32
    static LARGE_INTEGER frequency = {0};
    static bool frequency_initialized = false;

    if (!frequency_initialized) {
        QueryPerformanceFrequency(&frequency);
        frequency_initialized = true;
    }

    LARGE_INTEGER counter;
    QueryPerformanceCounter(&counter);

    return (uint64_t)((counter.QuadPart * 1000000000ULL) / frequency.QuadPart);
#else
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
#endif
}

/* ====== Serialization Functions ====== */

/* Serialize PerformanceTestData to bytes */
static size_t serialize_performance_data(const PerformanceTestData* data, uint8_t* buffer, size_t buffer_size) {
    size_t total_size = 8 + 8 + 8 + data->data_len;

    if (buffer_size < total_size) {
        fprintf(stderr, "Buffer too small for serialization\n");
        return 0;
    }

    size_t offset = 0;

    /* seq_num (little-endian) */
    buffer[offset++] = (uint8_t)(data->seq_num & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 8) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 16) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 24) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 32) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 40) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 48) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 56) & 0xFF);

    /* timestamp (little-endian) */
    buffer[offset++] = (uint8_t)(data->timestamp & 0xFF);
    buffer[offset++] = (uint8_t)((data->timestamp >> 8) & 0xFF);
    buffer[offset++] = (uint8_t)((data->timestamp >> 16) & 0xFF);
    buffer[offset++] = (uint8_t)((data->timestamp >> 24) & 0xFF);
    buffer[offset++] = (uint8_t)((data->timestamp >> 32) & 0xFF);
    buffer[offset++] = (uint8_t)((data->timestamp >> 40) & 0xFF);
    buffer[offset++] = (uint8_t)((data->timestamp >> 48) & 0xFF);
    buffer[offset++] = (uint8_t)((data->timestamp >> 56) & 0xFF);

    /* data_len (little-endian) */
    uint64_t data_len_64 = data->data_len;
    buffer[offset++] = (uint8_t)(data_len_64 & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 8) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 16) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 24) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 32) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 40) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 48) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 56) & 0xFF);

    /* data */
    memcpy(buffer + offset, data->data, data->data_len);
    offset += data->data_len;

    return offset;
}

/* Serialize LatencyTestData to bytes */
static size_t serialize_latency_data(const LatencyTestData* data, uint8_t* buffer, size_t buffer_size) {
    size_t total_size = 8 + 8 + 8 + 8 + data->data_len;

    if (buffer_size < total_size) {
        fprintf(stderr, "Buffer too small for serialization\n");
        return 0;
    }

    size_t offset = 0;

    /* seq_num */
    buffer[offset++] = (uint8_t)(data->seq_num & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 8) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 16) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 24) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 32) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 40) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 48) & 0xFF);
    buffer[offset++] = (uint8_t)((data->seq_num >> 56) & 0xFF);

    /* send_timestamp */
    buffer[offset++] = (uint8_t)(data->send_timestamp & 0xFF);
    buffer[offset++] = (uint8_t)((data->send_timestamp >> 8) & 0xFF);
    buffer[offset++] = (uint8_t)((data->send_timestamp >> 16) & 0xFF);
    buffer[offset++] = (uint8_t)((data->send_timestamp >> 24) & 0xFF);
    buffer[offset++] = (uint8_t)((data->send_timestamp >> 32) & 0xFF);
    buffer[offset++] = (uint8_t)((data->send_timestamp >> 40) & 0xFF);
    buffer[offset++] = (uint8_t)((data->send_timestamp >> 48) & 0xFF);
    buffer[offset++] = (uint8_t)((data->send_timestamp >> 56) & 0xFF);

    /* echo_timestamp */
    buffer[offset++] = (uint8_t)(data->echo_timestamp & 0xFF);
    buffer[offset++] = (uint8_t)((data->echo_timestamp >> 8) & 0xFF);
    buffer[offset++] = (uint8_t)((data->echo_timestamp >> 16) & 0xFF);
    buffer[offset++] = (uint8_t)((data->echo_timestamp >> 24) & 0xFF);
    buffer[offset++] = (uint8_t)((data->echo_timestamp >> 32) & 0xFF);
    buffer[offset++] = (uint8_t)((data->echo_timestamp >> 40) & 0xFF);
    buffer[offset++] = (uint8_t)((data->echo_timestamp >> 48) & 0xFF);
    buffer[offset++] = (uint8_t)((data->echo_timestamp >> 56) & 0xFF);

    /* data_len */
    uint64_t data_len_64 = data->data_len;
    buffer[offset++] = (uint8_t)(data_len_64 & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 8) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 16) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 24) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 32) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 40) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 48) & 0xFF);
    buffer[offset++] = (uint8_t)((data_len_64 >> 56) & 0xFF);

    /* data */
    memcpy(buffer + offset, data->data, data->data_len);
    offset += data->data_len;

    return offset;
}

/* Optimized: Extract only send_timestamp without memory allocation (zero-copy) */
static inline int deserialize_latency_timestamp_fast(const uint8_t* buffer, size_t buffer_size, uint64_t* send_timestamp) {
    if (buffer_size < 16) {  /* Only need seq_num(8) + send_timestamp(8) */
        return -1;
    }

    /* Skip seq_num (8 bytes), read send_timestamp directly using memcpy (little-endian) */
    memcpy(send_timestamp, buffer + 8, sizeof(uint64_t));
    return 0;
}

/* ====== Listener Callbacks ====== */

static void on_publication_matched(
    Int2DdsDataWriter* writer,
    const Int2DdsPublicationMatchedStatus* status,
    Int2DdsUserContext ctx
) {
    (void)writer;
    (void)ctx;

    if (status->current_count > 0) {
        printf("Subscriber matched! (total: %d, current: %d)\n",
               status->total_count, status->current_count);
    } else {
        printf("No subscribers matched.\n");
    }
}

static void on_data_available_latency_echo(
    Int2DdsDataReader* reader,
    Int2DdsUserContext ctx
) {
    (void)ctx;
    uint64_t current_time = get_current_time_ns();

    if (g_latency_buffer == NULL || g_latency_buffer_size == 0) {
        fprintf(stderr, "ERROR: g_latency_buffer not allocated\n");
        return;
    }

    uint8_t* buffer = g_latency_buffer;
    size_t buffer_size = g_latency_buffer_size;
    size_t data_size = 0;
    bool valid_data = false;

    while (1) {
        Int2DdsRet ret = int2dds_take(reader, buffer, buffer_size, &data_size, &valid_data);
        if (ret == INT2DDS_RET_NO_DATA) {
            break;
        } else if (ret != INT2DDS_RET_OK || !valid_data) {
            continue;
        }

        /* Fast path: extract only send_timestamp */
        uint64_t send_timestamp;
        if (deserialize_latency_timestamp_fast(buffer, data_size, &send_timestamp) != 0) {
            continue;
        }

        /* Calculate round-trip latency */
        uint64_t round_trip_ns = current_time - send_timestamp;
        double latency_ns = (double)round_trip_ns / 2.0;  /* One-way latency */

        g_latency_stats.latency_samples++;
        g_latency_stats.sum_latency += latency_ns;

        if (g_latency_stats.latency_samples == 1) {
            g_latency_stats.min_latency = latency_ns;
            g_latency_stats.max_latency = latency_ns;
        } else {
            if (g_latency_stats.min_latency < 0 || latency_ns < g_latency_stats.min_latency) {
                g_latency_stats.min_latency = latency_ns;
            }
            if (latency_ns > g_latency_stats.max_latency) {
                g_latency_stats.max_latency = latency_ns;
            }
        }
    }
}

/* ====== Helper Functions ====== */

static void print_usage(const char *progname) {
    printf("Usage: %s [OPTIONS]\n", progname);
    printf("Options:\n");
    printf("  -m, --mode MODE        Test mode: throughput, latency, local_latency (default: throughput)\n");
    printf("  -d, --domain-id ID     Domain ID (default: 0)\n");
    printf("  -s, --data-size SIZE   Data size in bytes (default: 1024)\n");
    printf("  -t, --time SECONDS     Execution time in seconds (default: 30)\n");
    printf("  -r, --reliability MODE Reliability mode: reliable or besteffort (default: besteffort)\n");
    printf("  -z, --hz RATE          Rate limit in Hz (optional)\n");
    printf("  -b, --batch-size SIZE  Batch size for throughput test (default: 1)\n");
    printf("  -c, --count COUNT      Number of latency samples to send (optional)\n");
    printf("  -i, --interval MS      Latency test interval in milliseconds (default: 100)\n");
    printf("  -h, --help             Show this help message\n");
    printf("\n");
    printf("Test modes:\n");
    printf("  throughput, thr, 1     Throughput test mode\n");
    printf("  latency, lat, 2       Latency test mode (echo-based)\n");
    printf("  local_latency, local, ll, 3  Local latency test mode (one-way)\n");
}

static void parse_args(int argc, char *argv[], publisher_args_t *args) {
    /* Set defaults */
    strcpy(args->test_mode, "throughput");
    args->domain_id = 0;
    args->data_len = 1024;
    args->execution_time = 30;
    strcpy(args->reliability, "besteffort");
    args->hz = 0.0;
    args->batch_size = 1;
    args->latency_count = 0;
    args->latency_interval_ms = 100;

#ifdef _WIN32
    /* Simple argument parsing for Windows */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--mode") == 0 || strcmp(argv[i], "-m") == 0) {
            if (i + 1 < argc) {
                strncpy(args->test_mode, argv[++i], sizeof(args->test_mode) - 1);
            }
        } else if (strcmp(argv[i], "--domain-id") == 0 || strcmp(argv[i], "-d") == 0) {
            if (i + 1 < argc) {
                args->domain_id = (int32_t)atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--data-size") == 0 || strcmp(argv[i], "-s") == 0) {
            if (i + 1 < argc) {
                args->data_len = (size_t)atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--time") == 0 || strcmp(argv[i], "-t") == 0) {
            if (i + 1 < argc) {
                args->execution_time = (uint64_t)atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--reliability") == 0 || strcmp(argv[i], "-r") == 0) {
            if (i + 1 < argc) {
                strncpy(args->reliability, argv[++i], sizeof(args->reliability) - 1);
            }
        } else if (strcmp(argv[i], "--hz") == 0 || strcmp(argv[i], "-z") == 0) {
            if (i + 1 < argc) {
                args->hz = atof(argv[++i]);
            }
        } else if (strcmp(argv[i], "--batch-size") == 0 || strcmp(argv[i], "-b") == 0) {
            if (i + 1 < argc) {
                args->batch_size = (size_t)atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--count") == 0 || strcmp(argv[i], "-c") == 0) {
            if (i + 1 < argc) {
                args->latency_count = (uint64_t)atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--interval") == 0 || strcmp(argv[i], "-i") == 0) {
            if (i + 1 < argc) {
                args->latency_interval_ms = (uint64_t)atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "-h") == 0) {
            print_usage(argv[0]);
            exit(0);
        }
    }
#else
    static struct option long_options[] = {
        {"mode", required_argument, 0, 'm'},
        {"domain-id", required_argument, 0, 'd'},
        {"data-size", required_argument, 0, 's'},
        {"time", required_argument, 0, 't'},
        {"reliability", required_argument, 0, 'r'},
        {"hz", required_argument, 0, 'z'},
        {"batch-size", required_argument, 0, 'b'},
        {"count", required_argument, 0, 'c'},
        {"interval", required_argument, 0, 'i'},
        {"help", no_argument, 0, 'h'},
        {0, 0, 0, 0}
    };

    int option_index = 0;
    int c;

    while ((c = getopt_long(argc, argv, "m:d:s:t:r:z:b:c:i:h", long_options, &option_index)) != -1) {
        switch (c) {
            case 'm':
                strncpy(args->test_mode, optarg, sizeof(args->test_mode) - 1);
                break;
            case 'd':
                args->domain_id = (int32_t)atoi(optarg);
                break;
            case 's':
                args->data_len = (size_t)atoi(optarg);
                break;
            case 't':
                args->execution_time = (uint64_t)atoi(optarg);
                break;
            case 'r':
                strncpy(args->reliability, optarg, sizeof(args->reliability) - 1);
                break;
            case 'z':
                args->hz = atof(optarg);
                break;
            case 'b':
                args->batch_size = (size_t)atoi(optarg);
                break;
            case 'c':
                args->latency_count = (uint64_t)atoi(optarg);
                break;
            case 'i':
                args->latency_interval_ms = (uint64_t)atoi(optarg);
                break;
            case 'h':
                print_usage(argv[0]);
                exit(0);
            default:
                print_usage(argv[0]);
                exit(1);
        }
    }
#endif
}

/* ====== Throughput Test ====== */

static void run_throughput_test(const publisher_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    performance_stats_t stats = {0};
    struct timespec start_time, end_time, last_report_time;
    uint64_t seq_num = 0;
    uint64_t last_report_count = 0;
    struct timespec next_send_time = {0};
    bool use_hz = (args->hz > 0.0);

    printf("\n=== Throughput Test (Publisher) ===\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", args->reliability);
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    printf("  Batch size: %zu\n", args->batch_size);
    if (use_hz) {
        printf("  Rate limit: %.1f Hz\n", args->hz);
    }
    printf("\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "perftest_publisher", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create publisher */
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    /* Create topic */
    ret = int2dds_create_topic(participant, "throughput_test_topic", "PerformanceData", NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* Create QoS */
    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    /* Set reliability */
    ret = int2dds_datawriter_qos_set_reliability(
        qos,
        strcmp(args->reliability, "reliable") == 0 ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        1000000000  /* 1 second */
    );
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    /* Set history */
    ret = int2dds_datawriter_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set history: %d\n", ret);
        goto cleanup;
    }

    /* Create listener */
    Int2DdsDataWriterListener listener = {
        .on_publication_matched = on_publication_matched,
        .on_offered_deadline_missed = NULL,
        .on_offered_incompatible_qos = NULL,
        .on_liveliness_lost = NULL,
        .user_context = NULL
    };

    /* Create writer with listener */
    ret = int2dds_create_datawriter_with_listener(publisher, topic, qos, &listener,
                                                    INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for subscriber to connect...\n");

    /* Wait for subscriber match */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datawriter(waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach writer to waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_wait(waitset, 60000000000ULL);  /* 60 seconds timeout */
    if (ret == INT2DDS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for subscriber\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    printf("Subscriber connected. Starting throughput test...\n");

    /* Allocate payload data */
    uint8_t* payload = (uint8_t*)malloc(args->data_len);
    if (payload == NULL) {
        fprintf(stderr, "Failed to allocate payload\n");
        goto cleanup;
    }
    memset(payload, 0xAA, args->data_len);

    /* Allocate serialization buffer */
    size_t buffer_size = 24 + args->data_len;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (buffer == NULL) {
        fprintf(stderr, "Failed to allocate buffer\n");
        free(payload);
        goto cleanup;
    }

    clock_gettime(CLOCK_MONOTONIC, &start_time);
    stats.start_time = start_time;
    last_report_time = start_time;

    if (use_hz) {
        next_send_time = start_time;
    }

    while (running) {
        clock_gettime(CLOCK_MONOTONIC, &end_time);
        double elapsed = (end_time.tv_sec - start_time.tv_sec) +
                        (end_time.tv_nsec - start_time.tv_nsec) / 1e9;

        if (elapsed >= args->execution_time) {
            break;
        }

        /* Create sample */
        PerformanceTestData sample = {
            .seq_num = seq_num,
            .timestamp = get_current_time_ns(),
            .data = payload,
            .data_len = args->data_len
        };

        /* Serialize */
        size_t serialized_size = serialize_performance_data(&sample, buffer, buffer_size);
        if (serialized_size == 0) {
            fprintf(stderr, "Serialization failed\n");
            break;
        }

        /* Write */
        ret = int2dds_write(writer, buffer, serialized_size);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Write failed: %d\n", ret);
            break;
        }

        stats.total_sent++;
        stats.total_bytes += args->data_len;
        seq_num++;

        /* Hz-based rate limiting */
        if (use_hz) {
            double target_interval = 1.0 / args->hz;
            next_send_time.tv_sec += (time_t)target_interval;
            next_send_time.tv_nsec += (long)((target_interval - (time_t)target_interval) * 1e9);

            if (next_send_time.tv_nsec >= 1000000000) {
                next_send_time.tv_sec++;
                next_send_time.tv_nsec -= 1000000000;
            }

            struct timespec now;
            clock_gettime(CLOCK_MONOTONIC, &now);
            if (now.tv_sec < next_send_time.tv_sec ||
                (now.tv_sec == next_send_time.tv_sec && now.tv_nsec < next_send_time.tv_nsec)) {
                struct timespec sleep_time = {
                    .tv_sec = next_send_time.tv_sec - now.tv_sec,
                    .tv_nsec = next_send_time.tv_nsec - now.tv_nsec
                };
                if (sleep_time.tv_nsec < 0) {
                    sleep_time.tv_sec--;
                    sleep_time.tv_nsec += 1000000000;
                }
                nanosleep(&sleep_time, NULL);
            }
        }

        /* Periodic reporting */
        double report_elapsed = (end_time.tv_sec - last_report_time.tv_sec) +
                               (end_time.tv_nsec - last_report_time.tv_nsec) / 1e9;
        if (report_elapsed >= 5.0) {
            double interval_elapsed = report_elapsed;
            uint64_t interval_sent = stats.total_sent - last_report_count;

            double avg_rate = stats.total_sent / elapsed;
            double interval_rate = interval_sent / interval_elapsed;
            double avg_mbps = (stats.total_sent * args->data_len * 8.0) / (elapsed * 1e6);
            double interval_mbps = (interval_sent * args->data_len * 8.0) / (interval_elapsed * 1e6);

            printf("[%.1fs] Sent: %" PRIu64 ", Avg: %.0f msg/s (%.2f Mbps), "
                   "Interval: %.0f msg/s (%.2f Mbps)\n",
                   elapsed, stats.total_sent, avg_rate, avg_mbps, interval_rate, interval_mbps);

            last_report_time = end_time;
            last_report_count = stats.total_sent;
        }
    }

    stats.end_time = end_time;
    double final_elapsed = (end_time.tv_sec - start_time.tv_sec) +
                          (end_time.tv_nsec - start_time.tv_nsec) / 1e9;
    double final_rate = stats.total_sent / final_elapsed;
    double mbps = (stats.total_sent * args->data_len * 8.0) / (final_elapsed * 1e6);

    printf("\n=== Throughput Test Publisher Results ===\n");
    printf("Total samples sent: %" PRIu64 "\n", stats.total_sent);
    printf("Test duration: %.2f seconds\n", final_elapsed);
    printf("Send rate: %.2f msg/s\n", final_rate);
    printf("Throughput: %.2f Mbps\n", mbps);

    /* Save results to CSV file */
    char filename[256];
    snprintf(filename, sizeof(filename), "publisher_throughput_results_%zub_%.0fhz.csv",
             args->data_len, args->hz);

    FILE *csv_file = fopen(filename, "a");
    if (csv_file) {
        fseek(csv_file, 0, SEEK_END);
        if (ftell(csv_file) == 0) {
            fprintf(csv_file, "Timestamp,Total_Sent_Messages,Messages_Per_Second,Throughput_Mbps\n");
        }

        uint64_t timestamp = get_current_time_ns() / 1000000;
        fprintf(csv_file, "%" PRIu64 ",%" PRIu64 ",%.2f,%.2f\n",
                timestamp, stats.total_sent, final_rate, mbps);

        fclose(csv_file);
        printf("Results saved to: %s\n", filename);
    }

    free(payload);
    free(buffer);

cleanup:
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ====== Latency Test ====== */

static void run_latency_test(const publisher_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsTopic* echo_topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataWriterQos* writer_qos = NULL;
    Int2DdsDataReaderQos* reader_qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    struct timespec start_time, end_time, last_report_time;
    uint64_t last_report_sent = 0;

    printf("\n=== Latency Test (Publisher) ===\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", args->reliability);
    if (args->latency_count > 0) {
        printf("  Latency count: %" PRIu64 " samples\n", args->latency_count);
    } else {
        printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    }
    if (args->hz > 0.0) {
        printf("  Rate limit: %.1f Hz\n", args->hz);
    } else {
        printf("  Latency interval: %" PRIu64 " ms\n", args->latency_interval_ms);
    }
    printf("\n");

    /* Reset global latency stats */
    memset(&g_latency_stats, 0, sizeof(g_latency_stats));
    g_latency_stats.min_latency = -1.0;

    /* Allocate dynamic buffer for latency callback */
    g_latency_buffer_size = args->data_len + 64;
    g_latency_buffer = (uint8_t*)malloc(g_latency_buffer_size);
    if (g_latency_buffer == NULL) {
        fprintf(stderr, "Failed to allocate latency buffer\n");
        return;
    }

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "perftest_latency_publisher", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create publisher */
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    /* Create subscriber (for receiving echoes) */
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    /* Create topics */
    ret = int2dds_create_topic(participant, "latency_test_topic", "LatencyTestData", NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(participant, "latency_echo_topic", "LatencyTestData", NULL, &echo_topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create echo topic: %d\n", ret);
        goto cleanup;
    }

    /* Create writer QoS */
    ret = int2dds_datawriter_qos_create_default(&writer_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create writer QoS: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datawriter_qos_set_reliability(
        writer_qos,
        strcmp(args->reliability, "reliable") == 0 ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        1000000000
    );
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set writer reliability: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datawriter_qos_set_history(writer_qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set writer history: %d\n", ret);
        goto cleanup;
    }

    /* Create writer listener */
    Int2DdsDataWriterListener writer_listener = {
        .on_publication_matched = on_publication_matched,
        .on_offered_deadline_missed = NULL,
        .on_offered_incompatible_qos = NULL,
        .on_liveliness_lost = NULL,
        .user_context = NULL
    };

    /* Create writer */
    ret = int2dds_create_datawriter_with_listener(publisher, topic, writer_qos, &writer_listener,
                                                    INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    /* Create reader QoS */
    ret = int2dds_datareader_qos_create_default(&reader_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create reader QoS: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datareader_qos_set_reliability(
        reader_qos,
        strcmp(args->reliability, "reliable") == 0 ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT
    );
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reader reliability: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datareader_qos_set_history(reader_qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reader history: %d\n", ret);
        goto cleanup;
    }

    /* Create reader listener */
    Int2DdsDataReaderListener reader_listener = {
        .on_data_available = on_data_available_latency_echo,
        .on_subscription_matched = NULL,
        .on_requested_deadline_missed = NULL,
        .on_requested_incompatible_qos = NULL,
        .on_sample_lost = NULL,
        .on_liveliness_changed = NULL,
        .user_context = NULL
    };

    /* Create reader (for echoes) */
    ret = int2dds_create_datareader_with_listener(subscriber, echo_topic, reader_qos, &reader_listener,
                                                    INT2DDS_STATUS_DATA_AVAILABLE, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for subscriber to connect...\n");

    /* Wait for subscriber match */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datawriter(waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach writer to waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_wait(waitset, 60000000000ULL);
    if (ret == INT2DDS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for subscriber\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    printf("Subscriber connected. Starting latency test...\n");

    /* Allocate payload data */
    uint8_t* payload = (uint8_t*)malloc(args->data_len);
    if (payload == NULL) {
        fprintf(stderr, "Failed to allocate payload\n");
        goto cleanup;
    }
    memset(payload, 0xAA, args->data_len);

    /* Allocate serialization buffer */
    size_t buffer_size = 32 + args->data_len;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (buffer == NULL) {
        fprintf(stderr, "Failed to allocate buffer\n");
        free(payload);
        goto cleanup;
    }

    clock_gettime(CLOCK_MONOTONIC, &start_time);
    last_report_time = start_time;

    uint64_t seq_num = 0;
    struct timespec next_send_time = start_time;
    bool use_precise_timing = (args->hz >= 10.0);

    if (args->latency_count > 0) {
        /* Count-based mode */
        printf("Sending %" PRIu64 " latency samples (max %" PRIu64 " seconds)...\n",
               args->latency_count, args->execution_time);

        while (seq_num < args->latency_count && running) {
            clock_gettime(CLOCK_MONOTONIC, &end_time);
            double elapsed = (end_time.tv_sec - start_time.tv_sec) +
                            (end_time.tv_nsec - start_time.tv_nsec) / 1e9;

            if (elapsed >= args->execution_time) {
                break;
            }

            LatencyTestData sample = {
                .seq_num = seq_num,
                .send_timestamp = get_current_time_ns(),
                .echo_timestamp = 0,
                .data = payload,
                .data_len = args->data_len
            };

            size_t serialized_size = serialize_latency_data(&sample, buffer, buffer_size);
            if (serialized_size == 0) {
                fprintf(stderr, "Serialization failed\n");
                break;
            }

            ret = int2dds_write(writer, buffer, serialized_size);
            if (ret != INT2DDS_RET_OK) {
                fprintf(stderr, "Write failed: %d\n", ret);
                break;
            }

            seq_num++;

            /* Periodic reporting */
            double report_elapsed = (end_time.tv_sec - last_report_time.tv_sec) +
                                   (end_time.tv_nsec - last_report_time.tv_nsec) / 1e9;

            if (report_elapsed >= 5.0) {
                double total_elapsed = elapsed;
                uint64_t interval_sent = seq_num - last_report_sent;

                double avg_rate = total_elapsed > 0 ? seq_num / total_elapsed : 0.0;
                double interval_rate = report_elapsed > 0 ? interval_sent / report_elapsed : 0.0;

                if (g_latency_stats.latency_samples > 0) {
                    double avg_latency_ns = g_latency_stats.sum_latency / g_latency_stats.latency_samples;
                    double avg_latency_ms = avg_latency_ns / 1000000.0;
                    double min_latency_ms = g_latency_stats.min_latency / 1000000.0;
                    double max_latency_ms = g_latency_stats.max_latency / 1000000.0;
                    printf("[%.1fs] Sent: %" PRIu64 ", Avg: %.0f msg/s, Interval: %.0f msg/s, "
                           "Avg Latency: %.3fms, Min: %.3fms, Max: %.3fms\n",
                           total_elapsed, seq_num, avg_rate, interval_rate,
                           avg_latency_ms, min_latency_ms, max_latency_ms);
                } else {
                    printf("[%.1fs] Sent: %" PRIu64 ", Avg: %.0f msg/s, Interval: %.0f msg/s\n",
                           total_elapsed, seq_num, avg_rate, interval_rate);
                }

                last_report_time = end_time;
                last_report_sent = seq_num;
            }

            /* Rate limiting */
            if (args->hz > 0.0) {
                if (use_precise_timing) {
                    double target_interval = 1.0 / args->hz;
                    next_send_time.tv_sec += (time_t)target_interval;
                    next_send_time.tv_nsec += (long)((target_interval - (time_t)target_interval) * 1e9);

                    if (next_send_time.tv_nsec >= 1000000000) {
                        next_send_time.tv_sec++;
                        next_send_time.tv_nsec -= 1000000000;
                    }

                    struct timespec now;
                    clock_gettime(CLOCK_MONOTONIC, &now);
                    if (now.tv_sec < next_send_time.tv_sec ||
                        (now.tv_sec == next_send_time.tv_sec && now.tv_nsec < next_send_time.tv_nsec)) {
                        struct timespec sleep_time = {
                            .tv_sec = next_send_time.tv_sec - now.tv_sec,
                            .tv_nsec = next_send_time.tv_nsec - now.tv_nsec
                        };
                        if (sleep_time.tv_nsec < 0) {
                            sleep_time.tv_sec--;
                            sleep_time.tv_nsec += 1000000000;
                        }
                        nanosleep(&sleep_time, NULL);
                    }
                } else {
                    double target_interval = 1.0 / args->hz;
                    int sleep_us = (int)(target_interval * 1000000);
                    if (sleep_us > 0) {
                        usleep(sleep_us);
                    }
                }
            } else {
                usleep((unsigned int)(args->latency_interval_ms * 1000));
            }
        }
    } else {
        /* Time-based mode */
        while (running) {
            clock_gettime(CLOCK_MONOTONIC, &end_time);
            double elapsed = (end_time.tv_sec - start_time.tv_sec) +
                            (end_time.tv_nsec - start_time.tv_nsec) / 1e9;

            if (elapsed >= args->execution_time) {
                break;
            }

            LatencyTestData sample = {
                .seq_num = seq_num,
                .send_timestamp = get_current_time_ns(),
                .echo_timestamp = 0,
                .data = payload,
                .data_len = args->data_len
            };

            size_t serialized_size = serialize_latency_data(&sample, buffer, buffer_size);
            if (serialized_size == 0) {
                fprintf(stderr, "Serialization failed\n");
                break;
            }

            ret = int2dds_write(writer, buffer, serialized_size);
            if (ret != INT2DDS_RET_OK) {
                fprintf(stderr, "Write failed: %d\n", ret);
                break;
            }

            seq_num++;

            /* Periodic reporting */
            double report_elapsed = (end_time.tv_sec - last_report_time.tv_sec) +
                                   (end_time.tv_nsec - last_report_time.tv_nsec) / 1e9;

            if (report_elapsed >= 5.0) {
                double total_elapsed = elapsed;
                uint64_t interval_sent = seq_num - last_report_sent;

                double avg_rate = total_elapsed > 0 ? seq_num / total_elapsed : 0.0;
                double interval_rate = report_elapsed > 0 ? interval_sent / report_elapsed : 0.0;

                if (g_latency_stats.latency_samples > 0) {
                    double avg_latency_ns = g_latency_stats.sum_latency / g_latency_stats.latency_samples;
                    double avg_latency_ms = avg_latency_ns / 1000000.0;
                    double min_latency_ms = g_latency_stats.min_latency / 1000000.0;
                    double max_latency_ms = g_latency_stats.max_latency / 1000000.0;
                    printf("[%.1fs] Sent: %" PRIu64 ", Avg: %.0f msg/s, Interval: %.0f msg/s, "
                           "Avg Latency: %.3fms, Min: %.3fms, Max: %.3fms\n",
                           total_elapsed, seq_num, avg_rate, interval_rate,
                           avg_latency_ms, min_latency_ms, max_latency_ms);
                } else {
                    printf("[%.1fs] Sent: %" PRIu64 ", Avg: %.0f msg/s, Interval: %.0f msg/s\n",
                           total_elapsed, seq_num, avg_rate, interval_rate);
                }

                last_report_time = end_time;
                last_report_sent = seq_num;
            }

            /* Rate limiting */
            if (args->hz > 0.0) {
                if (use_precise_timing) {
                    double target_interval = 1.0 / args->hz;
                    next_send_time.tv_sec += (time_t)target_interval;
                    next_send_time.tv_nsec += (long)((target_interval - (time_t)target_interval) * 1e9);

                    if (next_send_time.tv_nsec >= 1000000000) {
                        next_send_time.tv_sec++;
                        next_send_time.tv_nsec -= 1000000000;
                    }

                    struct timespec now;
                    clock_gettime(CLOCK_MONOTONIC, &now);
                    if (now.tv_sec < next_send_time.tv_sec ||
                        (now.tv_sec == next_send_time.tv_sec && now.tv_nsec < next_send_time.tv_nsec)) {
                        struct timespec sleep_time = {
                            .tv_sec = next_send_time.tv_sec - now.tv_sec,
                            .tv_nsec = next_send_time.tv_nsec - now.tv_nsec
                        };
                        if (sleep_time.tv_nsec < 0) {
                            sleep_time.tv_sec--;
                            sleep_time.tv_nsec += 1000000000;
                        }
                        nanosleep(&sleep_time, NULL);
                    }
                } else {
                    double target_interval = 1.0 / args->hz;
                    int sleep_us = (int)(target_interval * 1000000);
                    if (sleep_us > 0) {
                        usleep(sleep_us);
                    }
                }
            } else {
                usleep((unsigned int)(args->latency_interval_ms * 1000));
            }
        }
    }

    /* Wait for final echoes */
    usleep(1000000);  /* 1 second */

    clock_gettime(CLOCK_MONOTONIC, &end_time);
    double final_elapsed = (end_time.tv_sec - start_time.tv_sec) +
                          (end_time.tv_nsec - start_time.tv_nsec) / 1e9;

    printf("\n=== Latency Test Publisher Results ===\n");
    printf("Total samples sent: %" PRIu64 "\n", seq_num);
    printf("Test duration: %.2f seconds\n", final_elapsed);

    if (g_latency_stats.latency_samples > 0) {
        double avg_latency_ns = g_latency_stats.sum_latency / g_latency_stats.latency_samples;
        double avg_latency_ms = avg_latency_ns / 1000000.0;
        double max_latency_ms = g_latency_stats.max_latency / 1000000.0;
        printf("Latency samples: %" PRIu64 "\n", g_latency_stats.latency_samples);
        printf("Average latency: %.3f ms\n", avg_latency_ms);
        if (g_latency_stats.min_latency >= 0) {
            double min_latency_ms = g_latency_stats.min_latency / 1000000.0;
            printf("Minimum latency: %.3f ms\n", min_latency_ms);
        }
        printf("Maximum latency: %.3f ms\n", max_latency_ms);

        /* Save results to CSV file */
        char filename[256];
        if (args->hz > 0.0) {
            snprintf(filename, sizeof(filename), "publisher_latency_results_%zub_%.0fhz.csv",
                     args->data_len, args->hz);
        } else {
            snprintf(filename, sizeof(filename), "publisher_latency_results_%zub_%dms.csv",
                     args->data_len, (int)args->latency_interval_ms);
        }

        FILE *csv_file = fopen(filename, "a");
        if (csv_file) {
            fseek(csv_file, 0, SEEK_END);
            if (ftell(csv_file) == 0) {
                fprintf(csv_file, "Timestamp,Average_Latency_ms,Min_Latency_ms,Max_Latency_ms\n");
            }

            uint64_t timestamp = get_current_time_ns() / 1000000;
            if (g_latency_stats.min_latency >= 0) {
                double min_latency_ms = g_latency_stats.min_latency / 1000000.0;
                fprintf(csv_file, "%" PRIu64 ",%.2f,%.3f,%.2f\n",
                        timestamp, avg_latency_ms, min_latency_ms, max_latency_ms);
            } else {
                fprintf(csv_file, "%" PRIu64 ",%.2f,N/A,%.2f\n",
                        timestamp, avg_latency_ms, max_latency_ms);
            }

            fclose(csv_file);
            printf("Results saved to: %s\n", filename);
        }
    } else {
        printf("No latency samples received\n");
    }

    free(payload);
    free(buffer);

cleanup:
    if (g_latency_buffer) {
        free(g_latency_buffer);
        g_latency_buffer = NULL;
        g_latency_buffer_size = 0;
    }

    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ====== Local Latency Test ====== */

static void run_local_latency_test(const publisher_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    performance_stats_t stats = {0};
    struct timespec start_time, end_time, last_report_time;
    uint64_t seq_num = 0;
    uint64_t last_report_count = 0;
    struct timespec next_send_time = {0};
    bool use_hz = (args->hz > 0.0);

    printf("\n=== Local Latency Test (Publisher) ===\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", args->reliability);
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    if (use_hz) {
        printf("  Rate limit: %.1f Hz\n", args->hz);
    } else {
        printf("  Rate: unlimited\n");
    }
    printf("\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "perftest_local_publisher", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create publisher */
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    /* Create topic */
    ret = int2dds_create_topic(participant, "local_latency_test_topic", "PerformanceData", NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* Create QoS */
    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    /* Set reliability */
    ret = int2dds_datawriter_qos_set_reliability(
        qos,
        strcmp(args->reliability, "reliable") == 0 ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        1000000000
    );
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    /* Set history */
    ret = int2dds_datawriter_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set history: %d\n", ret);
        goto cleanup;
    }

    /* Create listener */
    Int2DdsDataWriterListener listener = {
        .on_publication_matched = on_publication_matched,
        .on_offered_deadline_missed = NULL,
        .on_offered_incompatible_qos = NULL,
        .on_liveliness_lost = NULL,
        .user_context = NULL
    };

    /* Create writer with listener */
    ret = int2dds_create_datawriter_with_listener(publisher, topic, qos, &listener,
                                                    INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for subscriber to connect...\n");

    /* Wait for subscriber match */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datawriter(waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach writer to waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_wait(waitset, 60000000000ULL);
    if (ret == INT2DDS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for subscriber\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    printf("Subscriber connected. Starting local latency test...\n");

    /* Allocate payload data */
    uint8_t* payload = (uint8_t*)malloc(args->data_len);
    if (payload == NULL) {
        fprintf(stderr, "Failed to allocate payload\n");
        goto cleanup;
    }
    memset(payload, 0xAA, args->data_len);

    /* Allocate serialization buffer */
    size_t buffer_size = 24 + args->data_len;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (buffer == NULL) {
        fprintf(stderr, "Failed to allocate buffer\n");
        free(payload);
        goto cleanup;
    }

    clock_gettime(CLOCK_MONOTONIC, &start_time);
    stats.start_time = start_time;
    last_report_time = start_time;

    if (use_hz) {
        next_send_time = start_time;
    }

    while (running) {
        clock_gettime(CLOCK_MONOTONIC, &end_time);
        double elapsed = (end_time.tv_sec - start_time.tv_sec) +
                        (end_time.tv_nsec - start_time.tv_nsec) / 1e9;

        if (elapsed >= args->execution_time) {
            break;
        }

        /* Hz-based rate limiting */
        if (use_hz) {
            struct timespec now;
            clock_gettime(CLOCK_MONOTONIC, &now);
            if (now.tv_sec < next_send_time.tv_sec ||
                (now.tv_sec == next_send_time.tv_sec && now.tv_nsec < next_send_time.tv_nsec)) {
                struct timespec sleep_time = {
                    .tv_sec = next_send_time.tv_sec - now.tv_sec,
                    .tv_nsec = next_send_time.tv_nsec - now.tv_nsec
                };
                if (sleep_time.tv_nsec < 0) {
                    sleep_time.tv_sec--;
                    sleep_time.tv_nsec += 1000000000;
                }
                nanosleep(&sleep_time, NULL);
            }
        }

        /* Create sample */
        PerformanceTestData sample = {
            .seq_num = seq_num,
            .timestamp = get_current_time_ns(),
            .data = payload,
            .data_len = args->data_len
        };

        /* Serialize */
        size_t serialized_size = serialize_performance_data(&sample, buffer, buffer_size);
        if (serialized_size == 0) {
            fprintf(stderr, "Serialization failed\n");
            break;
        }

        /* Write */
        ret = int2dds_write(writer, buffer, serialized_size);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Write failed: %d\n", ret);
            break;
        }

        stats.total_sent++;
        stats.total_bytes += args->data_len;
        seq_num++;

        /* Hz-based rate limiting (calculate next send time) */
        if (use_hz) {
            double target_interval = 1.0 / args->hz;
            next_send_time.tv_sec += (time_t)target_interval;
            next_send_time.tv_nsec += (long)((target_interval - (time_t)target_interval) * 1e9);

            if (next_send_time.tv_nsec >= 1000000000) {
                next_send_time.tv_sec++;
                next_send_time.tv_nsec -= 1000000000;
            }
        }

        /* Periodic reporting */
        double report_elapsed = (end_time.tv_sec - last_report_time.tv_sec) +
                               (end_time.tv_nsec - last_report_time.tv_nsec) / 1e9;
        if (report_elapsed >= 5.0) {
            double interval_elapsed = report_elapsed;
            uint64_t interval_sent = stats.total_sent - last_report_count;

            double avg_rate = stats.total_sent / elapsed;
            double interval_rate = interval_sent / interval_elapsed;

            printf("[%.1fs] Sent: %" PRIu64 ", Avg: %.0f msg/s, "
                   "Interval: %.0f msg/s\n",
                   elapsed, stats.total_sent, avg_rate, interval_rate);

            last_report_time = end_time;
            last_report_count = stats.total_sent;
        }
    }

    stats.end_time = end_time;
    double final_elapsed = (end_time.tv_sec - start_time.tv_sec) +
                          (end_time.tv_nsec - start_time.tv_nsec) / 1e9;
    double final_rate = stats.total_sent / final_elapsed;

    printf("\n=== Local Latency Test Publisher Results ===\n");
    printf("Total samples sent: %" PRIu64 "\n", stats.total_sent);
    printf("Test duration: %.2f seconds\n", final_elapsed);
    printf("Send rate: %.2f msg/s\n", final_rate);

    /* Save results to CSV file */
    char filename[256];
    if (args->hz > 0.0) {
        snprintf(filename, sizeof(filename), "publisher_local_latency_results_%zub_%.0fhz.csv",
                args->data_len, args->hz);
    } else {
        snprintf(filename, sizeof(filename), "publisher_local_latency_results_%zub.csv",
                args->data_len);
    }

    FILE *csv_file = fopen(filename, "a");
    if (csv_file) {
        fseek(csv_file, 0, SEEK_END);
        if (ftell(csv_file) == 0) {
            fprintf(csv_file, "Timestamp,Total_Sent_Messages,Duration_Seconds,Messages_Per_Second\n");
        }

        uint64_t timestamp = get_current_time_ns() / 1000000;
        fprintf(csv_file, "%" PRIu64 ",%" PRIu64 ",%.2f,%.2f\n",
                timestamp, stats.total_sent, final_elapsed, final_rate);

        fclose(csv_file);
        printf("Results saved to: %s\n", filename);
    }

    free(payload);
    free(buffer);

cleanup:
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ====== Main ====== */

int main(int argc, char *argv[]) {
    publisher_args_t args;

    /* Set up signal handler */
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    parse_args(argc, argv, &args);

    printf("int2dds Performance Test Publisher\n");
    printf("===================================\n");
    printf("Test mode: %s\n", args.test_mode);

    if (strcmp(args.test_mode, "throughput") == 0 ||
        strcmp(args.test_mode, "thr") == 0 ||
        strcmp(args.test_mode, "1") == 0) {
        run_throughput_test(&args);
    } else if (strcmp(args.test_mode, "latency") == 0 ||
               strcmp(args.test_mode, "lat") == 0 ||
               strcmp(args.test_mode, "2") == 0) {
        run_latency_test(&args);
    } else if (strcmp(args.test_mode, "local_latency") == 0 ||
               strcmp(args.test_mode, "local") == 0 ||
               strcmp(args.test_mode, "ll") == 0 ||
               strcmp(args.test_mode, "3") == 0) {
        run_local_latency_test(&args);
    } else {
        printf("Invalid test mode '%s'. Supported modes: throughput, latency, local_latency\n", args.test_mode);
        printf("Defaulting to throughput test.\n");
        run_throughput_test(&args);
    }

    return 0;
}
