/**
 * int2dds FFI Performance Test Publisher
 *
 * This program measures DDS performance by publishing test data in different modes:
 * - Throughput: Measures message throughput and bandwidth
 * - Latency: Measures round-trip latency with echo from subscriber
 * - Local Latency: Measures local loopback latency (one-way)
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <time.h>

#ifdef _WIN32
#include <windows.h>
#define sleep_ms(ms) Sleep(ms)
#else
#include <unistd.h>
#include <sys/time.h>
#define sleep_ms(ms) usleep((ms) * 1000)
#endif

#include "int2dds-ffi.h"

/* Test mode enumeration */
typedef enum {
    MODE_THROUGHPUT,
    MODE_LATENCY,
    MODE_LOCAL_LATENCY
} TestMode;

/* Command-line arguments */
typedef struct {
    TestMode mode;
    size_t data_size;
    uint64_t execution_time;  /* seconds */
    uint64_t warmup_time;     /* warmup seconds (not measured) */
    double hz;                /* 0 = unlimited */
    int use_reliable;
    int32_t domain_id;
} PerfTestArgs;

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

/* Statistics tracking */
typedef struct {
    uint64_t total_samples;
    uint64_t bytes_sent;
    uint64_t start_time_ns;
    uint64_t end_time_ns;
    double* latency_samples;
    size_t latency_count;
    size_t latency_capacity;
} PerformanceStats;

/* Global state for listeners */
static volatile int g_subscriber_matched = 0;
static PerformanceStats g_latency_stats = {0};
static uint8_t* g_latency_buffer = NULL;  /* Dynamic buffer for latency callback */
static size_t g_latency_buffer_size = 0;

/* ====== Timing Functions ====== */

#ifdef _WIN32
uint64_t get_current_time_ns() {
    LARGE_INTEGER frequency, counter;
    QueryPerformanceFrequency(&frequency);
    QueryPerformanceCounter(&counter);
    return (uint64_t)((counter.QuadPart * 1000000000ULL) / frequency.QuadPart);
}
#else
uint64_t get_current_time_ns() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)(ts.tv_sec * 1000000000ULL + ts.tv_nsec);
}
#endif

/* ====== Serialization Functions ====== */

/* Serialize PerformanceTestData to bytes */
size_t serialize_performance_data(const PerformanceTestData* data, uint8_t* buffer, size_t buffer_size) {
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
size_t serialize_latency_data(const LatencyTestData* data, uint8_t* buffer, size_t buffer_size) {
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

/* Deserialize LatencyTestData from bytes */
int deserialize_latency_data(const uint8_t* buffer, size_t buffer_size, LatencyTestData* data) {
    if (buffer_size < 32) {
        return -1;
    }

    size_t offset = 0;

    /* seq_num */
    data->seq_num = (uint64_t)buffer[offset++];
    data->seq_num |= ((uint64_t)buffer[offset++]) << 8;
    data->seq_num |= ((uint64_t)buffer[offset++]) << 16;
    data->seq_num |= ((uint64_t)buffer[offset++]) << 24;
    data->seq_num |= ((uint64_t)buffer[offset++]) << 32;
    data->seq_num |= ((uint64_t)buffer[offset++]) << 40;
    data->seq_num |= ((uint64_t)buffer[offset++]) << 48;
    data->seq_num |= ((uint64_t)buffer[offset++]) << 56;

    /* send_timestamp */
    data->send_timestamp = (uint64_t)buffer[offset++];
    data->send_timestamp |= ((uint64_t)buffer[offset++]) << 8;
    data->send_timestamp |= ((uint64_t)buffer[offset++]) << 16;
    data->send_timestamp |= ((uint64_t)buffer[offset++]) << 24;
    data->send_timestamp |= ((uint64_t)buffer[offset++]) << 32;
    data->send_timestamp |= ((uint64_t)buffer[offset++]) << 40;
    data->send_timestamp |= ((uint64_t)buffer[offset++]) << 48;
    data->send_timestamp |= ((uint64_t)buffer[offset++]) << 56;

    /* echo_timestamp */
    data->echo_timestamp = (uint64_t)buffer[offset++];
    data->echo_timestamp |= ((uint64_t)buffer[offset++]) << 8;
    data->echo_timestamp |= ((uint64_t)buffer[offset++]) << 16;
    data->echo_timestamp |= ((uint64_t)buffer[offset++]) << 24;
    data->echo_timestamp |= ((uint64_t)buffer[offset++]) << 32;
    data->echo_timestamp |= ((uint64_t)buffer[offset++]) << 40;
    data->echo_timestamp |= ((uint64_t)buffer[offset++]) << 48;
    data->echo_timestamp |= ((uint64_t)buffer[offset++]) << 56;

    /* data_len */
    uint64_t data_len = (uint64_t)buffer[offset++];
    data_len |= ((uint64_t)buffer[offset++]) << 8;
    data_len |= ((uint64_t)buffer[offset++]) << 16;
    data_len |= ((uint64_t)buffer[offset++]) << 24;
    data_len |= ((uint64_t)buffer[offset++]) << 32;
    data_len |= ((uint64_t)buffer[offset++]) << 40;
    data_len |= ((uint64_t)buffer[offset++]) << 48;
    data_len |= ((uint64_t)buffer[offset++]) << 56;

    if (buffer_size < offset + data_len) {
        return -1;
    }

    data->data_len = (size_t)data_len;
    data->data = (uint8_t*)malloc(data->data_len);
    if (data->data == NULL) {
        return -1;
    }

    memcpy(data->data, buffer + offset, data->data_len);
    return 0;
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

void on_publication_matched(
    Int2DdsDataWriter* writer,
    const Int2DdsPublicationMatchedStatus* status,
    Int2DdsUserContext ctx
) {
    (void)writer;
    (void)ctx;
    if (status->current_count > 0) {
        printf("Subscriber matched! (total: %d, current: %d)\n", status->total_count, status->current_count);
        g_subscriber_matched = 1;
    } else {
        printf("Subscriber disconnected.\n");
        g_subscriber_matched = 0;
    }
}

void on_data_available_latency_echo(
    Int2DdsDataReader* reader,
    Int2DdsUserContext ctx
) {
    (void)ctx;

    if (g_latency_buffer == NULL || g_latency_buffer_size == 0) {
        fprintf(stderr, "[DEBUG] ERROR: g_latency_buffer not allocated\n");
        return;
    }

    /* Use global dynamic buffer */
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

        /* Fast path: extract only send_timestamp (no malloc, no memcpy of payload) */
        uint64_t send_timestamp;
        if (deserialize_latency_timestamp_fast(buffer, data_size, &send_timestamp) != 0) {
            continue;
        }

        /* Calculate RTT */
        uint64_t now = get_current_time_ns();
        uint64_t rtt_ns = now - send_timestamp;

        /* Store latency sample (one-way = RTT / 2) */
        if (g_latency_stats.latency_count >= g_latency_stats.latency_capacity) {
            size_t new_capacity = g_latency_stats.latency_capacity == 0 ? 10000 : g_latency_stats.latency_capacity * 2;
            double* new_samples = (double*)realloc(g_latency_stats.latency_samples, new_capacity * sizeof(double));
            if (new_samples == NULL) {
                fprintf(stderr, "Failed to allocate memory for latency samples\n");
                break;
            }
            g_latency_stats.latency_samples = new_samples;
            g_latency_stats.latency_capacity = new_capacity;
        }

        g_latency_stats.latency_samples[g_latency_stats.latency_count++] = (double)rtt_ns;
    }
}

/* ====== Statistics Functions ====== */

void calculate_latency_stats(const PerformanceStats* stats, double* avg_ns, double* min_ns, double* max_ns) {
    if (stats->latency_count == 0) {
        *avg_ns = 0.0;
        *min_ns = 0.0;
        *max_ns = 0.0;
        return;
    }

    double sum = 0.0;
    double min_val = stats->latency_samples[0];
    double max_val = stats->latency_samples[0];

    for (size_t i = 0; i < stats->latency_count; i++) {
        double val = stats->latency_samples[i];
        sum += val;
        if (val < min_val) min_val = val;
        if (val > max_val) max_val = val;
    }

    /* One-way latency = RTT / 2 */
    *avg_ns = (sum / stats->latency_count) / 2.0;
    *min_ns = min_val / 2.0;
    *max_ns = max_val / 2.0;
}

void save_latency_csv(const PerformanceStats* stats, const char* filename) {
    FILE* fp = fopen(filename, "w");
    if (fp == NULL) {
        fprintf(stderr, "Failed to open CSV file: %s\n", filename);
        return;
    }

    fprintf(fp, "sample_num,latency_ns,latency_us,latency_ms\n");

    for (size_t i = 0; i < stats->latency_count; i++) {
        double rtt = stats->latency_samples[i];
        double latency_ns = rtt / 2.0;
        double latency_us = latency_ns / 1000.0;
        double latency_ms = latency_ns / 1000000.0;
        fprintf(fp, "%zu,%.2f,%.2f,%.6f\n", i + 1, latency_ns, latency_us, latency_ms);
    }

    fclose(fp);
    printf("Latency data saved to: %s\n", filename);
}

/* ====== Test Functions ====== */

int run_throughput_test(const PerfTestArgs* args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    printf("\n=== Throughput Test (Publisher) ===\n");
    printf("  Data size: %zu bytes\n", args->data_size);
    printf("  Reliability: %s\n", args->use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("  Warmup time: %llu seconds\n", (unsigned long long)args->warmup_time);
    printf("  Execution time: %llu seconds\n", (unsigned long long)args->execution_time);
    printf("  Rate: %s\n", args->hz > 0 ? "limited" : "unlimited");
    if (args->hz > 0) {
        printf("  Target Hz: %.2f\n", args->hz);
    }
    printf("\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
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
    ret = int2dds_create_topic(participant, "throughput_test_topic", "PerformanceTestData", NULL, &topic);
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
        args->use_reliable ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        100000000  /* 100ms */
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
    ret = int2dds_create_datawriter_with_listener(publisher, topic, qos, &listener, INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
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

    /* Allocate payload data */
    uint8_t* payload = (uint8_t*)malloc(args->data_size);
    if (payload == NULL) {
        fprintf(stderr, "Failed to allocate payload\n");
        goto cleanup;
    }
    memset(payload, 0xAA, args->data_size);

    /* Allocate serialization buffer */
    size_t buffer_size = 24 + args->data_size;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (buffer == NULL) {
        fprintf(stderr, "Failed to allocate buffer\n");
        free(payload);
        goto cleanup;
    }

    /* Performance statistics */
    uint64_t total_samples = 0;
    uint64_t bytes_sent = 0;
    uint64_t seq_num = 0;

    /* Warmup phase */
    if (args->warmup_time > 0) {
        printf("Starting warmup phase (%llu seconds)...\n", (unsigned long long)args->warmup_time);
        uint64_t warmup_start = get_current_time_ns();
        uint64_t warmup_end = warmup_start + (args->warmup_time * 1000000000ULL);
        uint64_t warmup_next_send = warmup_start;
        uint64_t warmup_interval = args->hz > 0 ? (uint64_t)(1000000000.0 / args->hz) : 0;

        while (1) {
            uint64_t now = get_current_time_ns();
            if (now >= warmup_end) break;

            if (args->hz > 0 && now < warmup_next_send) continue;

            PerformanceTestData sample = {
                .seq_num = seq_num++,
                .timestamp = now,
                .data = payload,
                .data_len = args->data_size
            };

            size_t serialized_size = serialize_performance_data(&sample, buffer, buffer_size);
            if (serialized_size == 0) break;

            ret = int2dds_write(writer, buffer, serialized_size);
            if (ret != INT2DDS_RET_OK) break;

            if (args->hz > 0) warmup_next_send += warmup_interval;
        }
        printf("Warmup complete. Sent %llu samples during warmup.\n", (unsigned long long)seq_num);
        seq_num = 0;  /* Reset for actual test */
    }

    /* Start test */
    uint64_t start_time_ns = get_current_time_ns();
    uint64_t test_end_time = start_time_ns + (args->execution_time * 1000000000ULL);
    uint64_t last_report_time = start_time_ns;
    uint64_t last_report_count = 0;

    uint64_t next_send_time = start_time_ns;
    uint64_t interval_ns = args->hz > 0 ? (uint64_t)(1000000000.0 / args->hz) : 0;

    printf("Starting throughput test...\n");

    while (1) {
        uint64_t now = get_current_time_ns();

        if (now >= test_end_time) {
            break;
        }

        /* Rate limiting */
        if (args->hz > 0 && now < next_send_time) {
            continue;
        }

        /* Create sample */
        PerformanceTestData sample = {
            .seq_num = seq_num++,
            .timestamp = now,
            .data = payload,
            .data_len = args->data_size
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

        total_samples++;
        bytes_sent += serialized_size;

        /* Update next send time */
        if (args->hz > 0) {
            next_send_time += interval_ns;
        }

        /* Periodic progress (every 5 seconds) */
        now = get_current_time_ns();
        if (now - last_report_time >= 5000000000ULL) {
            uint64_t elapsed_ns = now - start_time_ns;
            double elapsed_sec = elapsed_ns / 1e9;
            uint64_t interval_sent = total_samples - last_report_count;
            double interval_elapsed = (now - last_report_time) / 1e9;

            double avg_rate = total_samples / elapsed_sec;
            double interval_rate = interval_sent / interval_elapsed;
            double avg_mbps = (bytes_sent * 8.0) / (elapsed_sec * 1e6);
            double interval_mbps = (interval_sent * args->data_size * 8.0) / (interval_elapsed * 1e6);

            printf("[%.1fs] Sent: %llu, Avg: %.0f msg/s (%.2f Mbps), Interval: %.0f msg/s (%.2f Mbps)\n",
                   elapsed_sec, (unsigned long long)total_samples, avg_rate, avg_mbps, interval_rate, interval_mbps);

            last_report_time = now;
            last_report_count = total_samples;
        }
    }

    uint64_t end_time_ns = get_current_time_ns();

    /* Final progress line */
    double final_elapsed = (end_time_ns - start_time_ns) / 1e9;
    double final_avg_rate = total_samples / final_elapsed;
    double final_avg_mbps = (bytes_sent * 8.0) / (final_elapsed * 1e6);
    printf("[%.1fs] Sent: %llu, Avg: %.0f msg/s (%.2f Mbps)\n",
           final_elapsed, (unsigned long long)total_samples, final_avg_rate, final_avg_mbps);

    /* Print final statistics */
    double duration_sec = final_elapsed;
    double msgs_per_sec = total_samples / duration_sec;
    double mbps = (bytes_sent * 8.0) / (duration_sec * 1e6);

    printf("\n=== Throughput Test Results ===\n");
    printf("Total samples sent: %llu\n", (unsigned long long)total_samples);
    printf("Total bytes sent: %llu\n", (unsigned long long)bytes_sent);
    printf("Duration: %.2f seconds\n", duration_sec);
    printf("Throughput: %.2f msg/s, %.2f Mbps\n", msgs_per_sec, mbps);

    free(payload);
    free(buffer);

cleanup:
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    return 0;
}

int run_latency_test(const PerfTestArgs* args) {
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

    printf("\n=== Latency Test (Publisher) ===\n");
    printf("  Data size: %zu bytes\n", args->data_size);
    printf("  Reliability: %s\n", args->use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("  Warmup time: %llu seconds\n", (unsigned long long)args->warmup_time);
    printf("  Execution time: %llu seconds\n", (unsigned long long)args->execution_time);
    printf("  Rate: %s\n", args->hz > 0 ? "limited" : "unlimited");
    if (args->hz > 0) {
        printf("  Target Hz: %.2f\n", args->hz);
    }
    printf("\n");

    /* Reset global latency stats */
    g_latency_stats.total_samples = 0;
    g_latency_stats.bytes_sent = 0;
    g_latency_stats.start_time_ns = 0;
    g_latency_stats.end_time_ns = 0;
    g_latency_stats.latency_count = 0;
    if (g_latency_stats.latency_samples != NULL) {
        free(g_latency_stats.latency_samples);
        g_latency_stats.latency_samples = NULL;
    }
    g_latency_stats.latency_capacity = 0;

    /* Allocate dynamic buffer for latency callback (header 32 bytes + payload + margin) */
    g_latency_buffer_size = args->data_size + 64;
    g_latency_buffer = (uint8_t*)malloc(g_latency_buffer_size);
    if (g_latency_buffer == NULL) {
        fprintf(stderr, "Failed to allocate latency buffer (%zu bytes)\n", g_latency_buffer_size);
        return 1;
    }
    printf("Allocated latency buffer: %zu bytes\n", g_latency_buffer_size);

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
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
        args->use_reliable ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        100000000
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
        args->use_reliable ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT
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

    /* Wait for subscriber match using waitset */
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
        fprintf(stderr, "Timeout waiting for subscriber (60 seconds)\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    printf("Subscriber connected!\n");

    /* Allocate payload data */
    uint8_t* payload = (uint8_t*)malloc(args->data_size);
    if (payload == NULL) {
        fprintf(stderr, "Failed to allocate payload\n");
        goto cleanup;
    }
    memset(payload, 0xAA, args->data_size);

    /* Allocate serialization buffer */
    size_t buffer_size = 32 + args->data_size;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (buffer == NULL) {
        fprintf(stderr, "Failed to allocate buffer\n");
        free(payload);
        goto cleanup;
    }

    /* Performance statistics */
    uint64_t seq_num = 0;

    /* Warmup phase */
    if (args->warmup_time > 0) {
        printf("Starting warmup phase (%llu seconds)...\n", (unsigned long long)args->warmup_time);
        uint64_t warmup_start = get_current_time_ns();
        uint64_t warmup_end = warmup_start + (args->warmup_time * 1000000000ULL);
        uint64_t warmup_next_send = warmup_start;
        uint64_t warmup_interval = args->hz > 0 ? (uint64_t)(1000000000.0 / args->hz) : 10000000;

        while (1) {
            uint64_t now = get_current_time_ns();
            if (now >= warmup_end) break;

            if (now < warmup_next_send) {
                sleep_ms(1);
                continue;
            }

            LatencyTestData sample = {
                .seq_num = seq_num++,
                .send_timestamp = get_current_time_ns(),
                .echo_timestamp = 0,
                .data = payload,
                .data_len = args->data_size
            };

            size_t serialized_size = serialize_latency_data(&sample, buffer, buffer_size);
            if (serialized_size == 0) break;

            ret = int2dds_write(writer, buffer, serialized_size);
            if (ret != INT2DDS_RET_OK) break;

            warmup_next_send += warmup_interval;
        }
        printf("Warmup complete. Sent %llu samples during warmup.\n", (unsigned long long)seq_num);

        /* Reset stats for actual test */
        seq_num = 0;
        g_latency_stats.latency_count = 0;
    }

    /* Start test */
    uint64_t start_time = get_current_time_ns();
    uint64_t test_end_time = start_time + (args->execution_time * 1000000000ULL);
    uint64_t last_report_time = start_time;
    uint64_t last_report_count = 0;

    uint64_t next_send_time = start_time;
    uint64_t interval_ns = args->hz > 0 ? (uint64_t)(1000000000.0 / args->hz) : 10000000;  /* Default 10ms */

    printf("Starting latency test...\n");

    while (1) {
        uint64_t now = get_current_time_ns();

        if (now >= test_end_time) {
            break;
        }

        /* Rate limiting */
        if (now < next_send_time) {
            sleep_ms(1);
            continue;
        }

        /* Create latency sample */
        LatencyTestData sample = {
            .seq_num = seq_num++,
            .send_timestamp = get_current_time_ns(),
            .echo_timestamp = 0,
            .data = payload,
            .data_len = args->data_size
        };

        /* Serialize */
        size_t serialized_size = serialize_latency_data(&sample, buffer, buffer_size);
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

        /* Update next send time */
        next_send_time += interval_ns;

        /* Periodic progress (every 5 seconds) */
        now = get_current_time_ns();
        if (now - last_report_time >= 5000000000ULL) {
            double elapsed_sec = (now - start_time) / 1e9;
            uint64_t interval_sent = seq_num - last_report_count;
            double interval_elapsed = (now - last_report_time) / 1e9;

            double avg_rate = seq_num / elapsed_sec;
            double interval_rate = interval_sent / interval_elapsed;

            /* Calculate current latency stats */
            if (g_latency_stats.latency_count > 0) {
                double sum = 0.0;
                double min_val = g_latency_stats.latency_samples[0];
                double max_val = g_latency_stats.latency_samples[0];

                for (size_t i = 0; i < g_latency_stats.latency_count; i++) {
                    double val = g_latency_stats.latency_samples[i];
                    sum += val;
                    if (val < min_val) min_val = val;
                    if (val > max_val) max_val = val;
                }

                double avg_latency = (sum / g_latency_stats.latency_count) / 2.0;  /* One-way */
                double min_latency = min_val / 2.0;
                double max_latency = max_val / 2.0;

                printf("[%.1fs] Sent: %llu, Avg: %.0f msg/s, Interval: %.0f msg/s, "
                       "Latency: %.2f ms (min: %.2f ms, max: %.2f ms)\n",
                       elapsed_sec, (unsigned long long)seq_num, avg_rate, interval_rate,
                       avg_latency / 1e6, min_latency / 1e6, max_latency / 1e6);
            } else {
                printf("[%.1fs] Sent: %llu, Avg: %.0f msg/s, Interval: %.0f msg/s\n",
                       elapsed_sec, (unsigned long long)seq_num, avg_rate, interval_rate);
            }

            last_report_time = now;
            last_report_count = seq_num;
        }
    }

    uint64_t end_time = get_current_time_ns();

    /* Print [30.0s] progress before collecting echoes */
    double elapsed_at_end = (end_time - start_time) / 1e9;
    if (g_latency_stats.latency_count > 0) {
        double sum = 0.0;
        double min_val = g_latency_stats.latency_samples[0];
        double max_val = g_latency_stats.latency_samples[0];

        for (size_t i = 0; i < g_latency_stats.latency_count; i++) {
            double val = g_latency_stats.latency_samples[i];
            sum += val;
            if (val < min_val) min_val = val;
            if (val > max_val) max_val = val;
        }

        double avg_latency = (sum / g_latency_stats.latency_count) / 2.0;
        double min_latency = min_val / 2.0;
        double max_latency = max_val / 2.0;

        printf("[%.1fs] Sent: %llu, Echoes: %zu, Latency: %.2f ms (min: %.2f ms, max: %.2f ms)\n",
               elapsed_at_end, (unsigned long long)seq_num, g_latency_stats.latency_count,
               avg_latency / 1e6, min_latency / 1e6, max_latency / 1e6);
    } else {
        printf("[%.1fs] Sent: %llu, Echoes: %zu\n",
               elapsed_at_end, (unsigned long long)seq_num, g_latency_stats.latency_count);
    }

    /* Wait 1 second for final echoes and show [31.0s] progress */
    printf("Collecting final echoes...\n");
    sleep_ms(1000);

    /* Print [31.0s] progress */
    uint64_t collect_end = get_current_time_ns();
    double final_elapsed = (collect_end - start_time) / 1e9;
    if (g_latency_stats.latency_count > 0) {
        double sum = 0.0;
        double min_val = g_latency_stats.latency_samples[0];
        double max_val = g_latency_stats.latency_samples[0];

        for (size_t i = 0; i < g_latency_stats.latency_count; i++) {
            double val = g_latency_stats.latency_samples[i];
            sum += val;
            if (val < min_val) min_val = val;
            if (val > max_val) max_val = val;
        }

        double avg_latency = (sum / g_latency_stats.latency_count) / 2.0;
        double min_latency = min_val / 2.0;
        double max_latency = max_val / 2.0;

        printf("[%.1fs] Sent: %llu, Echoes: %zu, Latency: %.2f ms (min: %.2f ms, max: %.2f ms)\n",
               final_elapsed, (unsigned long long)seq_num, g_latency_stats.latency_count,
               avg_latency / 1e6, min_latency / 1e6, max_latency / 1e6);
    } else {
        printf("[%.1fs] Sent: %llu, Echoes: %zu\n",
               final_elapsed, (unsigned long long)seq_num, g_latency_stats.latency_count);
    }

    /* Print final statistics */
    printf("\n=== Latency Test Results ===\n");
    printf("Latency requests sent: %llu\n", (unsigned long long)seq_num);
    printf("Latency responses received: %zu\n", g_latency_stats.latency_count);
    printf("Duration: %.2f seconds\n", (end_time - start_time) / 1e9);

    if (g_latency_stats.latency_count > 0) {
        double avg_ns, min_ns, max_ns;
        calculate_latency_stats(&g_latency_stats, &avg_ns, &min_ns, &max_ns);

        printf("Average latency: %.3f ms (%.2f us)\n", avg_ns / 1e6, avg_ns / 1e3);
        printf("Min latency: %.3f ms (%.2f us)\n", min_ns / 1e6, min_ns / 1e3);
        printf("Max latency: %.3f ms (%.2f us)\n", max_ns / 1e6, max_ns / 1e3);

        /* Save to CSV */
        char filename[256];
        snprintf(filename, sizeof(filename), "publisher_latency_results_%zub_%.0fhz.csv",
                 args->data_size, args->hz);
        save_latency_csv(&g_latency_stats, filename);
    } else {
        printf("No latency responses received!\n");
    }

    free(payload);
    free(buffer);

cleanup:
    /* Free latency buffer */
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

    return 0;
}

int run_local_latency_test(const PerfTestArgs* args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    printf("\n=== Local Latency Test (Publisher) ===\n");
    printf("  Data size: %zu bytes\n", args->data_size);
    printf("  Reliability: %s\n", args->use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("  Warmup time: %llu seconds\n", (unsigned long long)args->warmup_time);
    printf("  Execution time: %llu seconds\n", (unsigned long long)args->execution_time);
    printf("  Rate: %s\n", args->hz > 0 ? "limited" : "unlimited");
    if (args->hz > 0) {
        printf("  Target Hz: %.2f\n", args->hz);
    }
    printf("\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
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
    ret = int2dds_create_topic(participant, "local_latency_test_topic", "PerformanceTestData", NULL, &topic);
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
        args->use_reliable ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT,
        100000000
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
    ret = int2dds_create_datawriter_with_listener(publisher, topic, qos, &listener, INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
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

    /* Allocate payload data */
    uint8_t* payload = (uint8_t*)malloc(args->data_size);
    if (payload == NULL) {
        fprintf(stderr, "Failed to allocate payload\n");
        goto cleanup;
    }
    memset(payload, 0xAA, args->data_size);

    /* Allocate serialization buffer */
    size_t buffer_size = 24 + args->data_size;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (buffer == NULL) {
        fprintf(stderr, "Failed to allocate buffer\n");
        free(payload);
        goto cleanup;
    }

    /* Performance statistics */
    uint64_t total_samples = 0;
    uint64_t seq_num = 0;

    /* Warmup phase */
    if (args->warmup_time > 0) {
        printf("Starting warmup phase (%llu seconds)...\n", (unsigned long long)args->warmup_time);
        uint64_t warmup_start = get_current_time_ns();
        uint64_t warmup_end = warmup_start + (args->warmup_time * 1000000000ULL);
        uint64_t warmup_next_send = warmup_start;
        uint64_t warmup_interval = args->hz > 0 ? (uint64_t)(1000000000.0 / args->hz) : 0;

        while (1) {
            uint64_t now = get_current_time_ns();
            if (now >= warmup_end) break;

            if (args->hz > 0 && now < warmup_next_send) continue;

            PerformanceTestData sample = {
                .seq_num = seq_num++,
                .timestamp = now,
                .data = payload,
                .data_len = args->data_size
            };

            size_t serialized_size = serialize_performance_data(&sample, buffer, buffer_size);
            if (serialized_size == 0) break;

            ret = int2dds_write(writer, buffer, serialized_size);
            if (ret != INT2DDS_RET_OK) break;

            if (args->hz > 0) warmup_next_send += warmup_interval;
        }
        printf("Warmup complete. Sent %llu samples during warmup.\n", (unsigned long long)seq_num);
        seq_num = 0;  /* Reset for actual test */
    }

    /* Start test */
    uint64_t start_time_ns = get_current_time_ns();
    uint64_t test_end_time = start_time_ns + (args->execution_time * 1000000000ULL);
    uint64_t last_report_time = start_time_ns;
    uint64_t last_report_count = 0;

    uint64_t next_send_time = start_time_ns;
    uint64_t interval_ns = args->hz > 0 ? (uint64_t)(1000000000.0 / args->hz) : 0;

    printf("Starting local latency test...\n");

    while (1) {
        uint64_t now = get_current_time_ns();

        if (now >= test_end_time) {
            break;
        }

        /* Rate limiting */
        if (args->hz > 0 && now < next_send_time) {
            continue;
        }

        /* Create sample */
        PerformanceTestData sample = {
            .seq_num = seq_num++,
            .timestamp = now,
            .data = payload,
            .data_len = args->data_size
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

        total_samples++;

        /* Update next send time */
        if (args->hz > 0) {
            next_send_time += interval_ns;
        }

        /* Periodic progress (every 5 seconds) */
        now = get_current_time_ns();
        if (now - last_report_time >= 5000000000ULL) {
            uint64_t elapsed_ns = now - start_time_ns;
            double elapsed_sec = elapsed_ns / 1e9;
            uint64_t interval_sent = total_samples - last_report_count;
            double interval_elapsed = (now - last_report_time) / 1e9;

            double avg_rate = total_samples / elapsed_sec;
            double interval_rate = interval_sent / interval_elapsed;

            printf("[%.1fs] Sent: %llu, Avg: %.0f msg/s, Interval: %.0f msg/s\n",
                   elapsed_sec, (unsigned long long)total_samples, avg_rate, interval_rate);

            last_report_time = now;
            last_report_count = total_samples;
        }
    }

    uint64_t end_time_ns = get_current_time_ns();

    /* Print final statistics */
    double duration_sec = (end_time_ns - start_time_ns) / 1e9;
    double msgs_per_sec = total_samples / duration_sec;

    printf("\n=== Local Latency Test Results ===\n");
    printf("Total samples sent: %llu\n", (unsigned long long)total_samples);
    printf("Duration: %.2f seconds\n", duration_sec);
    printf("Send rate: %.2f msg/s\n", msgs_per_sec);

    free(payload);
    free(buffer);

cleanup:
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    return 0;
}

/* ====== Argument Parsing ====== */

void print_usage(const char* prog_name) {
    printf("Usage: %s [OPTIONS]\n", prog_name);
    printf("\nOptions:\n");
    printf("  --mode <mode>          Test mode: throughput, latency, local_latency (default: throughput)\n");
    printf("  --size <bytes>         Payload size in bytes (default: 1024)\n");
    printf("  --time <seconds>       Test duration in seconds (default: 30)\n");
    printf("  --warmup <seconds>     Warmup duration in seconds, not measured (default: 0)\n");
    printf("  --hz <rate>            Message rate in Hz, 0 for unlimited (default: 0)\n");
    printf("  --reliability <mode>   QoS reliability: best_effort or reliable (default: best_effort)\n");
    printf("  --domain <id>          DDS domain ID (default: 0)\n");
    printf("  --help                 Show this help message\n");
}

int parse_args(int argc, char** argv, PerfTestArgs* args) {
    /* Set defaults */
    args->mode = MODE_THROUGHPUT;
    args->data_size = 1024;
    args->execution_time = 30;
    args->warmup_time = 0;
    args->hz = 0.0;
    args->use_reliable = 0;  /* best_effort is default */
    args->domain_id = 0;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--help") == 0) {
            return -1;
        } else if (strcmp(argv[i], "--mode") == 0 && i + 1 < argc) {
            i++;
            if (strcmp(argv[i], "throughput") == 0) {
                args->mode = MODE_THROUGHPUT;
            } else if (strcmp(argv[i], "latency") == 0) {
                args->mode = MODE_LATENCY;
            } else if (strcmp(argv[i], "local_latency") == 0) {
                args->mode = MODE_LOCAL_LATENCY;
            } else {
                fprintf(stderr, "Unknown mode: %s\n", argv[i]);
                return -1;
            }
        } else if (strcmp(argv[i], "--size") == 0 && i + 1 < argc) {
            i++;
            args->data_size = (size_t)atoi(argv[i]);
        } else if (strcmp(argv[i], "--time") == 0 && i + 1 < argc) {
            i++;
            args->execution_time = (uint64_t)atoi(argv[i]);
        } else if (strcmp(argv[i], "--warmup") == 0 && i + 1 < argc) {
            i++;
            args->warmup_time = (uint64_t)atoi(argv[i]);
        } else if (strcmp(argv[i], "--hz") == 0 && i + 1 < argc) {
            i++;
            args->hz = atof(argv[i]);
        } else if (strcmp(argv[i], "--reliability") == 0 && i + 1 < argc) {
            i++;
            if (strcmp(argv[i], "reliable") == 0) {
                args->use_reliable = 1;
            } else if (strcmp(argv[i], "best_effort") == 0) {
                args->use_reliable = 0;
            } else {
                fprintf(stderr, "Unknown reliability: %s\n", argv[i]);
                return -1;
            }
        } else if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            i++;
            args->domain_id = atoi(argv[i]);
        } else {
            fprintf(stderr, "Unknown argument: %s\n", argv[i]);
            return -1;
        }
    }

    return 0;
}

/* ====== Main ====== */

int main(int argc, char** argv) {
    PerfTestArgs args;

    if (parse_args(argc, argv, &args) != 0) {
        print_usage(argv[0]);
        return 1;
    }

    printf("int2dds Performance Test Publisher\n");
    printf("===================================\n");

    int result = 0;
    switch (args.mode) {
        case MODE_THROUGHPUT:
            result = run_throughput_test(&args);
            break;
        case MODE_LATENCY:
            result = run_latency_test(&args);
            break;
        case MODE_LOCAL_LATENCY:
            result = run_local_latency_test(&args);
            break;
        default:
            fprintf(stderr, "Unknown test mode\n");
            result = 1;
            break;
    }

    /* Cleanup global latency stats */
    if (g_latency_stats.latency_samples) {
        free(g_latency_stats.latency_samples);
    }

    return result;
}
