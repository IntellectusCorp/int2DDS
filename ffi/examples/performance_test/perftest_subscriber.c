/*
 * int2dds FFI Performance Test Subscriber
 *
 * Based on CycloneDDS performance test, converted to use int2dds FFI API.
 * This program receives test data and measures performance:
 * - Throughput: Measures message throughput, bandwidth, and packet loss
 * - Latency: Echoes received data back to publisher for latency measurement
 * - Local Latency: Receives data from local publisher and measures one-way latency
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
#define usleep(x) Sleep((x) / 1000)
#define sleep_ms(ms) Sleep(ms)
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
    uli.QuadPart -= 116444736000000000ULL;
    ts->tv_sec = (time_t)(uli.QuadPart / 10000000ULL);
    ts->tv_nsec = (long)((uli.QuadPart % 10000000ULL) * 100);
    return 0;
}
#else
#include <unistd.h>
#define sleep_ms(ms) usleep((ms) * 1000)
#endif

#include "int2dds-ffi.h"

#define MAX_SAMPLES 100

/* ====== Data Structures ====== */

/* Note: With the new TypeDescriptor API, we don't need to manually define
 * data structures. The types are defined at runtime using TypeDescriptor.
 *
 * PerformanceData type:
 *   - seq_num: uint64
 *   - timestamp: uint64
 *   - data: sequence<uint8>
 *
 * LatencyTestData type:
 *   - seq_num: uint64
 *   - send_timestamp: uint64
 *   - echo_timestamp: uint64
 *   - data: sequence<uint8>
 */

/* Performance statistics */
typedef struct {
    uint64_t total_received;
    uint64_t total_bytes;
    uint64_t lost_samples;
    uint64_t expected_seq;
    struct timespec start_time;
    struct timespec last_report_time;
    bool out_of_order;
} performance_stats_t;

/* Latency statistics */
typedef struct {
    uint64_t total_latency_samples;
    double total_latency_us;
    double min_latency_us;
    double max_latency_us;
    double avg_latency_us;
    struct timespec start_time;
} latency_stats_t;

/* Local latency statistics */
typedef struct {
    uint64_t total_samples;
    uint64_t latency_samples_count;
    double sum_latency_ns;
    double min_latency_ns;
    double max_latency_ns;
    struct timespec start_time;
    struct timespec last_message_time;
    bool first_sample_received;
} local_latency_stats_t;

/* Command-line arguments */
typedef struct {
    char test_mode[32];
    int32_t domain_id;
    size_t data_len;
    uint64_t execution_time;
    char reliability[32];
    double hz;
} subscriber_args_t;

/* Global state */
static volatile bool running = true;
static performance_stats_t stats = {0};
static latency_stats_t latency_stats = {0};
static local_latency_stats_t local_latency_stats = {0};
static Int2DdsDataWriter* g_echo_writer = NULL;
static size_t g_data_size = 1024;

/* Data containers for callbacks */
static Int2DdsData* g_recv_data = NULL;       /* For throughput and local_latency callbacks */
static Int2DdsData* g_echo_data = NULL;       /* For latency echo callback */

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

/* ====== TypeDescriptor Helper Functions ====== */

/* Create TypeDescriptor for PerformanceData (throughput and local_latency tests) */
static Int2DdsTypeDescriptor* create_performance_type_descriptor(uint32_t max_data_size) {
    Int2DdsTypeDescriptor* type_desc = NULL;
    Int2DdsRet ret;

    ret = int2dds_type_descriptor_create("PerformanceData", &type_desc);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create performance type descriptor: %d\n", ret);
        return NULL;
    }

    ret = int2dds_type_descriptor_add_u64(type_desc, "seq_num", false);
    if (ret != INT2DDS_RET_OK) goto error;

    ret = int2dds_type_descriptor_add_u64(type_desc, "timestamp", false);
    if (ret != INT2DDS_RET_OK) goto error;

    ret = int2dds_type_descriptor_add_sequence(type_desc, "data", UInt8, max_data_size, false);
    if (ret != INT2DDS_RET_OK) goto error;

    return type_desc;

error:
    fprintf(stderr, "Failed to add field to performance type descriptor: %d\n", ret);
    int2dds_type_descriptor_delete(type_desc);
    return NULL;
}

/* Create TypeDescriptor for LatencyTestData (latency echo test) */
static Int2DdsTypeDescriptor* create_latency_type_descriptor(uint32_t max_data_size) {
    Int2DdsTypeDescriptor* type_desc = NULL;
    Int2DdsRet ret;

    ret = int2dds_type_descriptor_create("LatencyTestData", &type_desc);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create latency type descriptor: %d\n", ret);
        return NULL;
    }

    ret = int2dds_type_descriptor_add_u64(type_desc, "seq_num", false);
    if (ret != INT2DDS_RET_OK) goto error;

    ret = int2dds_type_descriptor_add_u64(type_desc, "send_timestamp", false);
    if (ret != INT2DDS_RET_OK) goto error;

    ret = int2dds_type_descriptor_add_u64(type_desc, "echo_timestamp", false);
    if (ret != INT2DDS_RET_OK) goto error;

    ret = int2dds_type_descriptor_add_sequence(type_desc, "data", UInt8, max_data_size, false);
    if (ret != INT2DDS_RET_OK) goto error;

    return type_desc;

error:
    fprintf(stderr, "Failed to add field to latency type descriptor: %d\n", ret);
    int2dds_type_descriptor_delete(type_desc);
    return NULL;
}

/* ====== Listener Callbacks ====== */

static void on_subscription_matched(
    Int2DdsDataReader* reader,
    const Int2DdsSubscriptionMatchedStatus* status,
    Int2DdsUserContext ctx
) {
    (void)reader;
    (void)ctx;

    if (status->current_count > 0) {
        printf("Publisher matched! (total: %d, current: %d)\n",
               status->total_count, status->current_count);
    } else {
        printf("Publisher disconnected.\n");
    }
}

static void on_data_available_throughput(
    Int2DdsDataReader* reader,
    Int2DdsUserContext ctx
) {
    (void)ctx;

    if (!running) {
        return;
    }

    if (g_recv_data == NULL) {
        fprintf(stderr, "ERROR: g_recv_data not allocated\n");
        return;
    }

    bool valid_data = false;

    /* Read all available samples */
    while (1) {
        Int2DdsRet ret = int2dds_take(reader, g_recv_data, &valid_data);

        if (ret == INT2DDS_RET_NO_DATA) {
            break;
        } else if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to take data: %d\n", ret);
            break;
        }

        if (!valid_data) {
            continue;
        }

        /* Get values from data container */
        uint64_t seq_num = 0;
        uint32_t data_len = 0;

        int2dds_data_get_u64(g_recv_data, "seq_num", &seq_num);
        /* Get byte sequence length by calling get_bytes with NULL buffer */
        int2dds_data_get_bytes(g_recv_data, "data", NULL, 0, &data_len);

        /* Start timing on first message */
        if (stats.total_received == 0) {
            clock_gettime(CLOCK_MONOTONIC, &stats.start_time);
            stats.last_report_time = stats.start_time;
        }

        /* Process message */
        if (stats.out_of_order) {
            /* Out-of-order mode: count all samples */
            stats.total_received++;
            stats.total_bytes += data_len;
        } else {
            /* In-order mode: track expected sequence and detect lost samples */
            if (seq_num >= stats.expected_seq) {
                stats.lost_samples += seq_num - stats.expected_seq;
                stats.expected_seq = seq_num + 1;

                stats.total_received++;
                stats.total_bytes += data_len;
            }
        }

        /* Periodic reporting every 5 seconds */
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        double elapsed = (now.tv_sec - stats.last_report_time.tv_sec) +
                        (now.tv_nsec - stats.last_report_time.tv_nsec) / 1e9;

        if (elapsed >= 5.0) {
            double total_elapsed = (now.tv_sec - stats.start_time.tv_sec) +
                                  (now.tv_nsec - stats.start_time.tv_nsec) / 1e9;
            double rate = total_elapsed > 0 ? stats.total_received / total_elapsed : 0.0;
            double mbps = total_elapsed > 0 ? (stats.total_bytes * 8.0) / (total_elapsed * 1000000.0) : 0.0;
            double loss_rate = 0.0;

            if (stats.total_received > 0) {
                loss_rate = (double)stats.lost_samples / (stats.total_received + stats.lost_samples) * 100.0;
            }

            printf("[%.1fs] Received: %" PRIu64 " messages, Rate: %.0f msg/s (%.2f Mbps), Loss: %.2f%%\n",
                   total_elapsed, stats.total_received, rate, mbps, loss_rate);

            stats.last_report_time = now;
        }
    }
}

static void on_data_available_latency_echo(
    Int2DdsDataReader* reader,
    Int2DdsUserContext ctx
) {
    (void)ctx;

    if (!running) {
        return;
    }

    if (g_echo_writer == NULL) {
        return;
    }

    if (g_echo_data == NULL) {
        fprintf(stderr, "ERROR: g_echo_data not allocated\n");
        return;
    }

    bool valid_data = false;
    uint64_t sample_count = 0;

    /* Read all available samples */
    while (1) {
        Int2DdsRet ret = int2dds_take(reader, g_echo_data, &valid_data);

        if (ret == INT2DDS_RET_NO_DATA) {
            break;
        } else if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to take data: %d\n", ret);
            break;
        }

        if (!valid_data) {
            continue;
        }

        /* Set start time on first sample */
        if (latency_stats.total_latency_samples == 0) {
            clock_gettime(CLOCK_MONOTONIC, &latency_stats.start_time);
            stats.last_report_time = latency_stats.start_time;
        }

        /* Add echo timestamp and write back */
        int2dds_data_set_u64(g_echo_data, "echo_timestamp", get_current_time_ns());

        Int2DdsRet write_ret = int2dds_write(g_echo_writer, g_echo_data);
        if (write_ret == INT2DDS_RET_OK) {
            sample_count++;
        } else {
            fprintf(stderr, "Failed to write echo: %d\n", write_ret);
        }
    }

    /* Update total samples */
    if (sample_count > 0) {
        latency_stats.total_latency_samples += sample_count;
    }
}

static void on_data_available_local_latency(
    Int2DdsDataReader* reader,
    Int2DdsUserContext ctx
) {
    (void)ctx;

    if (!running) {
        return;
    }

    uint64_t receive_time_ns = get_current_time_ns();

    if (g_recv_data == NULL) {
        fprintf(stderr, "ERROR: g_recv_data not allocated\n");
        return;
    }

    bool valid_data = false;

    /* Read all available samples */
    while (1) {
        Int2DdsRet ret = int2dds_take(reader, g_recv_data, &valid_data);

        if (ret == INT2DDS_RET_NO_DATA) {
            break;
        } else if (ret != INT2DDS_RET_OK) {
            break;
        }

        if (!valid_data) {
            continue;
        }

        /* Get timestamp from data container */
        uint64_t timestamp = 0;
        int2dds_data_get_u64(g_recv_data, "timestamp", &timestamp);

        /* Set start_time on first sample */
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        if (!local_latency_stats.first_sample_received) {
            local_latency_stats.start_time = now;
            local_latency_stats.first_sample_received = true;
            printf("First local latency sample received. Starting measurements...\n");
        }

        local_latency_stats.last_message_time = now;

        /* Calculate one-way latency */
        uint64_t latency_ns = (receive_time_ns > timestamp) ?
                             (receive_time_ns - timestamp) : 0;

        local_latency_stats.total_samples++;
        local_latency_stats.latency_samples_count++;
        local_latency_stats.sum_latency_ns += latency_ns;

        if (local_latency_stats.min_latency_ns < 0 || latency_ns < local_latency_stats.min_latency_ns) {
            local_latency_stats.min_latency_ns = latency_ns;
        }
        if (latency_ns > local_latency_stats.max_latency_ns) {
            local_latency_stats.max_latency_ns = latency_ns;
        }

        /* Periodic reporting */
        double elapsed = (now.tv_sec - stats.last_report_time.tv_sec) +
                        (now.tv_nsec - stats.last_report_time.tv_nsec) / 1e9;

        if (elapsed >= 5.0) {
            double total_elapsed = (now.tv_sec - local_latency_stats.start_time.tv_sec) +
                                  (now.tv_nsec - local_latency_stats.start_time.tv_nsec) / 1e9;

            if (local_latency_stats.latency_samples_count > 0) {
                double avg_ns = local_latency_stats.sum_latency_ns / local_latency_stats.latency_samples_count;
                double min_ns = local_latency_stats.min_latency_ns >= 0 ? local_latency_stats.min_latency_ns : 0;
                double max_ns = local_latency_stats.max_latency_ns;

                printf("[%.1fs] Samples: %" PRIu64 ", Avg: %.3f ms, Min: %.3f ms, Max: %.3f ms\n",
                       total_elapsed, local_latency_stats.latency_samples_count,
                       avg_ns / 1000000.0, min_ns / 1000000.0, max_ns / 1000000.0);
            }

            stats.last_report_time = now;
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
    printf("  -z, --hz RATE          Rate (for CSV filename) (optional)\n");
    printf("  -h, --help             Show this help message\n");
    printf("\n");
    printf("Test modes:\n");
    printf("  throughput, thr, 1     Throughput test mode\n");
    printf("  latency, lat, 2       Latency test mode (echo service)\n");
    printf("  local_latency, local, ll, 3  Local latency test mode\n");
}

static void parse_args(int argc, char *argv[], subscriber_args_t *args) {
    /* Set defaults */
    strcpy(args->test_mode, "throughput");
    args->domain_id = 0;
    args->data_len = 1024;
    args->execution_time = 30;
    strcpy(args->reliability, "besteffort");
    args->hz = 0.0;

    /* Simple argument parsing */
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
        } else if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "-h") == 0) {
            print_usage(argv[0]);
            exit(0);
        }
    }
}

/* ====== Throughput Test ====== */

static void run_throughput_test(const subscriber_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;
    Int2DdsTypeDescriptor* type_desc = NULL;

    printf("\n=== Throughput Test (Subscriber) ===\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", args->reliability);
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    printf("\n");

    /* Initialize global stats */
    memset(&stats, 0, sizeof(stats));
    stats.expected_seq = 0;
    stats.out_of_order = false;
    g_data_size = args->data_len;

    /* Create TypeDescriptor */
    type_desc = create_performance_type_descriptor((uint32_t)args->data_len);
    if (type_desc == NULL) {
        fprintf(stderr, "Failed to create type descriptor\n");
        return;
    }

    /* Create data container for callback */
    ret = int2dds_data_create(type_desc, &g_recv_data);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create data container: %d\n", ret);
        int2dds_type_descriptor_delete(type_desc);
        return;
    }
    printf("Created data container for receiving\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        goto cleanup;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "perftest_subscriber", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create subscriber */
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    /* Create topic with TypeDescriptor */
    ret = int2dds_create_topic(participant, "throughput_test_topic", type_desc, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* Create QoS */
    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    /* Set reliability */
    ret = int2dds_datareader_qos_set_reliability(
        qos,
        strcmp(args->reliability, "reliable") == 0 ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT
    );
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    /* Set history */
    ret = int2dds_datareader_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set history: %d\n", ret);
        goto cleanup;
    }

    /* Create listener */
    Int2DdsDataReaderListener listener = {
        .on_data_available = on_data_available_throughput,
        .on_subscription_matched = on_subscription_matched,
        .on_sample_rejected = NULL,
        .on_liveliness_changed = NULL,
        .on_requested_deadline_missed = NULL,
        .on_requested_incompatible_qos = NULL,
        .on_sample_lost = NULL,
        .user_context = NULL
    };

    /* Create reader with listener */
    ret = int2dds_create_datareader_with_listener(subscriber, topic, qos, &listener,
                                                    INT2DDS_STATUS_DATA_AVAILABLE | INT2DDS_STATUS_SUBSCRIPTION_MATCHED,
                                                    &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for publisher to connect...\n");

    /* Wait for publisher match */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datareader(waitset, reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach reader to waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_wait(waitset, 60000000000ULL);
    if (ret == INT2DDS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for publisher\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    printf("Publisher connected. Waiting for data...\n");

    /* Run for specified duration */
    struct timespec test_start;
    clock_gettime(CLOCK_MONOTONIC, &test_start);
    uint64_t test_end_time_ns = (uint64_t)test_start.tv_sec * 1000000000ULL + test_start.tv_nsec +
                                (args->execution_time * 1000000000ULL);

    while (running) {
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        uint64_t now_ns = (uint64_t)now.tv_sec * 1000000000ULL + now.tv_nsec;

        /* Check execution time */
        if (now_ns >= test_end_time_ns) {
            printf("\nTest duration reached.\n");
            break;
        }

        sleep_ms(100);
    }

    /* Print final statistics */
    struct timespec end_time;
    clock_gettime(CLOCK_MONOTONIC, &end_time);
    sleep_ms(100);
    double duration_sec = (end_time.tv_sec - test_start.tv_sec) +
                         (end_time.tv_nsec - test_start.tv_nsec) / 1e9;
    double msgs_per_sec = duration_sec > 0 ? stats.total_received / duration_sec : 0.0;
    double mbps = duration_sec > 0 ? (stats.total_bytes * 8.0) / (duration_sec * 1e6) : 0.0;
    double loss_rate = (stats.total_received > 0) ?
                       (stats.lost_samples * 100.0 / (stats.total_received + stats.lost_samples)) : 0.0;

    printf("\n=== Performance Test Results ===\n");
    printf("Test duration: %.2f seconds\n", duration_sec);
    printf("Total samples received: %" PRIu64 "\n", stats.total_received);
    printf("Messages per second: %.2f\n", msgs_per_sec);
    printf("Throughput: %.2f Mbps\n", mbps);
    printf("Lost samples: %" PRIu64 "\n", stats.lost_samples);
    printf("Loss rate: %.4f%%\n", loss_rate);

    /* Save results to CSV */
    char filename[256];
    snprintf(filename, sizeof(filename), "subscriber_throughput_results_%zub_%.0fhz.csv",
             args->data_len, args->hz);

    FILE *csv_file = fopen(filename, "a");
    if (csv_file) {
        fseek(csv_file, 0, SEEK_END);
        if (ftell(csv_file) == 0) {
            fprintf(csv_file, "Timestamp,Total_Received_Messages,Messages_Per_Second,Lost_Messages,Loss_Rate_Percent,Throughput_Mbps\n");
        }

        uint64_t timestamp = get_current_time_ns() / 1000000;
        fprintf(csv_file, "%" PRIu64 ",%" PRIu64 ",%.0f,%" PRIu64 ",%.2f,%.2f\n",
                timestamp, stats.total_received, msgs_per_sec, stats.lost_samples, loss_rate, mbps);

        fclose(csv_file);
        printf("Results saved to: %s\n", filename);
    }

cleanup:
    if (g_recv_data) {
        int2dds_data_delete(g_recv_data);
        g_recv_data = NULL;
    }

    if (type_desc) int2dds_type_descriptor_delete(type_desc);

    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ====== Latency Test (Echo Service) ====== */

static void run_latency_test(const subscriber_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsTopic* echo_topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataReaderQos* reader_qos = NULL;
    Int2DdsDataWriterQos* writer_qos = NULL;
    Int2DdsWaitSet* waitset = NULL;
    Int2DdsTypeDescriptor* type_desc = NULL;

    printf("\n=== Latency Test (Subscriber/Echo) ===\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", args->reliability);
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    printf("\n");

    /* Reset global state */
    g_echo_writer = NULL;
    memset(&latency_stats, 0, sizeof(latency_stats));
    memset(&stats, 0, sizeof(stats));

    /* Create TypeDescriptor */
    type_desc = create_latency_type_descriptor((uint32_t)args->data_len);
    if (type_desc == NULL) {
        fprintf(stderr, "Failed to create type descriptor\n");
        return;
    }

    /* Create data container for echo callback */
    ret = int2dds_data_create(type_desc, &g_echo_data);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create data container: %d\n", ret);
        int2dds_type_descriptor_delete(type_desc);
        return;
    }
    printf("Created data container for echo\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        goto cleanup;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "perftest_latency_subscriber", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create subscriber */
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    /* Create publisher (for echoing) */
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    /* Create topics with TypeDescriptor */
    ret = int2dds_create_topic(participant, "latency_test_topic", type_desc, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(participant, "latency_echo_topic", type_desc, NULL, &echo_topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create echo topic: %d\n", ret);
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

    /* Create writer (for echoing) */
    ret = int2dds_create_datawriter(publisher, echo_topic, writer_qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    /* Store writer in global for echo callback */
    g_echo_writer = writer;

    /* Create reader listener */
    Int2DdsDataReaderListener reader_listener = {
        .on_data_available = on_data_available_latency_echo,
        .on_subscription_matched = on_subscription_matched,
        .on_requested_deadline_missed = NULL,
        .on_requested_incompatible_qos = NULL,
        .on_sample_lost = NULL,
        .on_liveliness_changed = NULL,
        .user_context = NULL
    };

    /* Create reader (for receiving requests) */
    ret = int2dds_create_datareader_with_listener(subscriber, topic, reader_qos, &reader_listener,
                                                    INT2DDS_STATUS_DATA_AVAILABLE | INT2DDS_STATUS_SUBSCRIPTION_MATCHED,
                                                    &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for publisher to connect...\n");

    /* Wait for publisher match */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datareader(waitset, reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach reader to waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_wait(waitset, 60000000000ULL);
    if (ret == INT2DDS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for publisher\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    printf("Publisher connected! Echoing latency requests...\n");

    /* Run for specified duration */
    struct timespec start_time, last_report_time;
    clock_gettime(CLOCK_MONOTONIC, &start_time);
    last_report_time = start_time;
    uint64_t last_report_count = 0;

    struct timespec end_time;
    uint64_t test_end_time_ns = (uint64_t)start_time.tv_sec * 1000000000ULL + start_time.tv_nsec +
                                (args->execution_time * 1000000000ULL);

    while (running) {
        clock_gettime(CLOCK_MONOTONIC, &end_time);
        uint64_t now_ns = (uint64_t)end_time.tv_sec * 1000000000ULL + end_time.tv_nsec;

        if (now_ns >= test_end_time_ns) {
            break;
        }

        /* Periodic progress (every 5 seconds) */
        double report_elapsed = (end_time.tv_sec - last_report_time.tv_sec) +
                               (end_time.tv_nsec - last_report_time.tv_nsec) / 1e9;
        if (report_elapsed >= 5.0) {
            double elapsed = (end_time.tv_sec - start_time.tv_sec) +
                            (end_time.tv_nsec - start_time.tv_nsec) / 1e9;
            double rate = elapsed > 0 ? latency_stats.total_latency_samples / elapsed : 0.0;
            printf("[%.1fs] Echoes sent: %" PRIu64 ", Rate: %.0f echo/s\n",
                   elapsed, latency_stats.total_latency_samples, rate);
            last_report_time = end_time;
        }

        sleep_ms(100);
    }

    /* Print final statistics */
    double duration = (end_time.tv_sec - start_time.tv_sec) +
                     (end_time.tv_nsec - start_time.tv_nsec) / 1e9;
    double rate = duration > 0 ? latency_stats.total_latency_samples / duration : 0.0;

    printf("\n=== Latency Echo Test Results ===\n");
    printf("Duration: %.2f seconds\n", duration);
    printf("Total echoes sent: %" PRIu64 "\n", latency_stats.total_latency_samples);
    printf("Average echo rate: %.0f echo/s\n", rate);
    printf("Echo service completed successfully\n");

cleanup:
    running = false;  /* Stop callbacks before cleanup */
    g_echo_writer = NULL;

    if (g_echo_data) {
        int2dds_data_delete(g_echo_data);
        g_echo_data = NULL;
    }

    if (type_desc) int2dds_type_descriptor_delete(type_desc);

    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ====== Local Latency Test ====== */

static void run_local_latency_test(const subscriber_args_t *args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;
    Int2DdsTypeDescriptor* type_desc = NULL;

    printf("\n=== Local Latency Test (Subscriber) ===\n");
    printf("  Data size: %zu bytes\n", args->data_len);
    printf("  Reliability: %s\n", args->reliability);
    printf("  Execution time: %" PRIu64 " seconds\n", args->execution_time);
    printf("\n");

    /* Initialize global stats */
    memset(&local_latency_stats, 0, sizeof(local_latency_stats));
    local_latency_stats.min_latency_ns = -1.0;
    memset(&stats, 0, sizeof(stats));
    g_data_size = args->data_len;

    /* Create TypeDescriptor */
    type_desc = create_performance_type_descriptor((uint32_t)args->data_len);
    if (type_desc == NULL) {
        fprintf(stderr, "Failed to create type descriptor\n");
        return;
    }

    /* Create data container for callback */
    ret = int2dds_data_create(type_desc, &g_recv_data);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create data container: %d\n", ret);
        int2dds_type_descriptor_delete(type_desc);
        return;
    }
    printf("Created data container for receiving\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        goto cleanup;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "perftest_local_subscriber", args->domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create subscriber */
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    /* Create topic with TypeDescriptor */
    ret = int2dds_create_topic(participant, "local_latency_test_topic", type_desc, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* Create QoS */
    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    /* Set reliability */
    ret = int2dds_datareader_qos_set_reliability(
        qos,
        strcmp(args->reliability, "reliable") == 0 ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT
    );
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    /* Set history */
    ret = int2dds_datareader_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set history: %d\n", ret);
        goto cleanup;
    }

    /* Create listener */
    Int2DdsDataReaderListener listener = {
        .on_data_available = on_data_available_local_latency,
        .on_subscription_matched = on_subscription_matched,
        .on_sample_rejected = NULL,
        .on_liveliness_changed = NULL,
        .on_requested_deadline_missed = NULL,
        .on_requested_incompatible_qos = NULL,
        .on_sample_lost = NULL,
        .user_context = NULL
    };

    /* Create reader with listener */
    ret = int2dds_create_datareader_with_listener(subscriber, topic, qos, &listener,
                                                    INT2DDS_STATUS_DATA_AVAILABLE | INT2DDS_STATUS_SUBSCRIPTION_MATCHED,
                                                    &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for publisher...\n");

    /* Wait for publisher match */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datareader(waitset, reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach reader to waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_wait(waitset, 60000000000ULL);
    if (ret == INT2DDS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for publisher\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    printf("Publisher connected. Waiting for data...\n");

    /* Run for specified duration */
    struct timespec test_start;
    clock_gettime(CLOCK_MONOTONIC, &test_start);
    uint64_t test_end_time_ns = (uint64_t)test_start.tv_sec * 1000000000ULL + test_start.tv_nsec +
                                (args->execution_time * 1000000000ULL);
    uint64_t timeout_threshold = 2000000000ULL;

    while (running) {
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        uint64_t now_ns = (uint64_t)now.tv_sec * 1000000000ULL + now.tv_nsec;

        /* Check execution time */
        if (now_ns >= test_end_time_ns) {
            printf("\nTest duration reached.\n");
            break;
        }

        /* Check for data timeout */
        if (local_latency_stats.first_sample_received) {
            uint64_t last_msg_ns = (uint64_t)local_latency_stats.last_message_time.tv_sec * 1000000000ULL +
                                   local_latency_stats.last_message_time.tv_nsec;
            if ((now_ns - last_msg_ns) > timeout_threshold) {
                printf("\n2 second timeout after last message. Stopping...\n");
                break;
            }
        }

        sleep_ms(100);
    }

    /* Print final statistics */
    struct timespec end_time;
    clock_gettime(CLOCK_MONOTONIC, &end_time);
    double duration_sec = (end_time.tv_sec - test_start.tv_sec) +
                         (end_time.tv_nsec - test_start.tv_nsec) / 1e9;

    printf("\n=== Local Latency Test Results ===\n");
    printf("Test duration: %.2f seconds\n", duration_sec);
    printf("Total samples: %" PRIu64 "\n", local_latency_stats.total_samples);

    if (local_latency_stats.latency_samples_count > 0) {
        double avg_ns = local_latency_stats.sum_latency_ns / local_latency_stats.latency_samples_count;
        double min_ns = local_latency_stats.min_latency_ns >= 0 ? local_latency_stats.min_latency_ns : 0;
        double max_ns = local_latency_stats.max_latency_ns;

        printf("Latency samples: %" PRIu64 "\n", local_latency_stats.latency_samples_count);
        printf("Average latency: %.3f ms\n", avg_ns / 1000000.0);
        printf("Min latency: %.3f ms\n", min_ns / 1000000.0);
        printf("Max latency: %.3f ms\n", max_ns / 1000000.0);

        /* Save results to CSV */
        char filename[256];
        if (args->hz > 0.0) {
            snprintf(filename, sizeof(filename), "subscriber_local_latency_results_%zub_%.0fhz.csv",
                    args->data_len, args->hz);
        } else {
            snprintf(filename, sizeof(filename), "subscriber_local_latency_results_%zub.csv",
                    args->data_len);
        }

        FILE *csv_file = fopen(filename, "a");
        if (csv_file) {
            fseek(csv_file, 0, SEEK_END);
            if (ftell(csv_file) == 0) {
                fprintf(csv_file, "Timestamp,Total_Samples,Avg_Latency_ms,Min_Latency_ms,Max_Latency_ms\n");
            }

            uint64_t timestamp = get_current_time_ns() / 1000000;
            fprintf(csv_file, "%" PRIu64 ",%" PRIu64 ",%.3f,%.3f,%.3f\n",
                    timestamp, local_latency_stats.latency_samples_count,
                    avg_ns / 1000000.0, min_ns / 1000000.0, max_ns / 1000000.0);

            fclose(csv_file);
            printf("Results saved to: %s\n", filename);
        }
    }

cleanup:
    if (g_recv_data) {
        int2dds_data_delete(g_recv_data);
        g_recv_data = NULL;
    }

    if (type_desc) int2dds_type_descriptor_delete(type_desc);

    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ====== Main ====== */

int main(int argc, char *argv[]) {
    subscriber_args_t args;

    /* Set up signal handler */
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    parse_args(argc, argv, &args);

    printf("int2dds Performance Test Subscriber\n");
    printf("====================================\n");
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
