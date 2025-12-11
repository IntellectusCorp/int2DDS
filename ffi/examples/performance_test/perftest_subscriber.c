/**
 * int2dds FFI Performance Test Subscriber
 *
 * This program receives test data and measures performance:
 * - Throughput: Measures message throughput, bandwidth, and packet loss
 * - Latency: Echoes received data back to publisher for latency measurement
 * - Local Latency: Receives data from local publisher and measures one-way latency
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
    uint64_t warmup_time;     /* warmup seconds (waits for warmup data) */
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
    uint64_t lost_samples;
    uint64_t bytes_received;
    uint64_t expected_seq_num;
    uint64_t last_seq_num;
    uint64_t last_message_time_ns;
    uint64_t start_time_ns;  /* Added for periodic reporting */
    uint64_t last_report_time_ns;  /* Added for periodic reporting */
    uint64_t last_report_count;  /* Added for periodic reporting */
    int first_sample_received;
    double* latency_samples;
    size_t latency_count;
    size_t latency_capacity;
} PerformanceStats;

/* Global state */
static volatile int g_publisher_matched = 0;
static volatile int g_should_stop = 0;
static PerformanceStats g_stats = {0};
static Int2DdsDataWriter* g_echo_writer = NULL;
static size_t g_data_size = 1024;  /* For throughput calculation */
static uint8_t* g_echo_buffer = NULL;  /* Dynamic buffer for echo callback */
static size_t g_echo_buffer_size = 0;

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

/* Deserialize PerformanceTestData from bytes */
int deserialize_performance_data(const uint8_t* buffer, size_t buffer_size, PerformanceTestData* data) {
    if (buffer_size < 24) {
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

    /* timestamp */
    data->timestamp = (uint64_t)buffer[offset++];
    data->timestamp |= ((uint64_t)buffer[offset++]) << 8;
    data->timestamp |= ((uint64_t)buffer[offset++]) << 16;
    data->timestamp |= ((uint64_t)buffer[offset++]) << 24;
    data->timestamp |= ((uint64_t)buffer[offset++]) << 32;
    data->timestamp |= ((uint64_t)buffer[offset++]) << 40;
    data->timestamp |= ((uint64_t)buffer[offset++]) << 48;
    data->timestamp |= ((uint64_t)buffer[offset++]) << 56;

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
    if (data->data == NULL && data->data_len > 0) {
        return -1;
    }

    if (data->data_len > 0) {
        memcpy(data->data, buffer + offset, data->data_len);
    }

    return 0;
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
    if (data->data == NULL && data->data_len > 0) {
        return -1;
    }

    if (data->data_len > 0) {
        memcpy(data->data, buffer + offset, data->data_len);
    }

    return 0;
}

/* Serialize LatencyTestData to bytes (for echo) */
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
    if (data->data_len > 0) {
        memcpy(buffer + offset, data->data, data->data_len);
        offset += data->data_len;
    }

    return offset;
}

/* ====== Listener Callbacks ====== */

void on_subscription_matched(
    Int2DdsDataReader* reader,
    const Int2DdsSubscriptionMatchedStatus* status,
    Int2DdsUserContext ctx
) {
    (void)reader;
    (void)ctx;
    if (status->current_count > 0) {
        printf("Publisher matched! (total: %d, current: %d)\n", status->total_count, status->current_count);
        g_publisher_matched = 1;
    } else {
        printf("Publisher disconnected.\n");
        g_publisher_matched = 0;
    }
}

void on_data_available_throughput(
    Int2DdsDataReader* reader,
    Int2DdsUserContext ctx
) {
    (void)ctx;

    if (g_should_stop) {
        return;
    }

    uint8_t buffer[65536];
    size_t data_size = 0;
    bool valid_data = false;

    /* Read all available samples */
    while (1) {
        Int2DdsRet ret = int2dds_take(reader, buffer, sizeof(buffer), &data_size, &valid_data);

        if (ret == INT2DDS_RET_NO_DATA) {
            break;
        } else if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to take data: %d\n", ret);
            break;
        }

        if (!valid_data) {
            continue;
        }

        /* Deserialize */
        PerformanceTestData sample = {0};
        if (deserialize_performance_data(buffer, data_size, &sample) != 0) {
            fprintf(stderr, "Failed to deserialize sample\n");
            continue;
        }

        /* First sample - start timing */
        if (!g_stats.first_sample_received) {
            g_stats.first_sample_received = 1;
            g_stats.expected_seq_num = sample.seq_num;
            g_stats.start_time_ns = get_current_time_ns();
            g_stats.last_report_time_ns = g_stats.start_time_ns;
            g_stats.last_report_count = 0;
            printf("First data sample received. Starting measurements...\n");
        }

        /* Track sequence numbers and detect loss */
        if (sample.seq_num >= g_stats.expected_seq_num) {
            g_stats.lost_samples += (sample.seq_num - g_stats.expected_seq_num);
            g_stats.expected_seq_num = sample.seq_num + 1;
        }

        if (sample.seq_num > g_stats.last_seq_num) {
            g_stats.last_seq_num = sample.seq_num;
        }

        g_stats.total_samples++;
        g_stats.bytes_received += data_size;
        g_stats.last_message_time_ns = get_current_time_ns();

        /* Periodic progress (every 5 seconds) */
        if (g_stats.last_message_time_ns - g_stats.last_report_time_ns >= 5000000000ULL) {
            uint64_t elapsed_ns = g_stats.last_message_time_ns - g_stats.start_time_ns;
            double elapsed_sec = elapsed_ns / 1e9;
            uint64_t interval_samples = g_stats.total_samples - g_stats.last_report_count;
            double interval_elapsed = (g_stats.last_message_time_ns - g_stats.last_report_time_ns) / 1e9;

            double avg_rate = g_stats.total_samples / elapsed_sec;
            double interval_rate = interval_samples / interval_elapsed;
            double avg_mbps = (g_stats.bytes_received * 8.0) / (elapsed_sec * 1e6);
            double interval_mbps = (interval_samples * g_data_size * 8.0) / (interval_elapsed * 1e6);
            double loss_rate = (g_stats.last_seq_num > 0) ?
                               (g_stats.lost_samples * 100.0 / g_stats.last_seq_num) : 0.0;

            printf("[%.1fs] Received: %llu, Avg: %.0f msg/s (%.2f Mbps), Interval: %.0f msg/s (%.2f Mbps), "
                   "Lost: %llu, Last seq: %llu, Loss rate: %.2f%%\n",
                   elapsed_sec, (unsigned long long)g_stats.total_samples, avg_rate, avg_mbps,
                   interval_rate, interval_mbps, (unsigned long long)g_stats.lost_samples,
                   (unsigned long long)g_stats.last_seq_num, loss_rate);

            g_stats.last_report_time_ns = g_stats.last_message_time_ns;
            g_stats.last_report_count = g_stats.total_samples;
        }

        /* Free deserialized data */
        if (sample.data) {
            free(sample.data);
        }
    }
}

void on_data_available_latency_echo(
    Int2DdsDataReader* reader,
    Int2DdsUserContext ctx
) {
    (void)ctx;

    if (g_echo_writer == NULL) {
        fprintf(stderr, "[DEBUG] ERROR: g_echo_writer is NULL in callback\n");
        return;
    }

    if (g_echo_buffer == NULL || g_echo_buffer_size == 0) {
        fprintf(stderr, "[DEBUG] ERROR: g_echo_buffer not allocated\n");
        return;
    }

    /* Use half of buffer for read, half for write */
    uint8_t* buffer = g_echo_buffer;
    uint8_t* echo_buffer = g_echo_buffer + (g_echo_buffer_size / 2);
    size_t buffer_size = g_echo_buffer_size / 2;

    size_t data_size = 0;
    bool valid_data = false;
    uint64_t sample_count = 0;

    /* Read all available samples */
    while (1) {
        Int2DdsRet ret = int2dds_take(reader, buffer, buffer_size, &data_size, &valid_data);

        if (ret == INT2DDS_RET_NO_DATA) {
            break;
        } else if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "[DEBUG] ERROR: int2dds_take failed with ret=%d, buffer_size=%zu, data_size=%zu\n",
                    ret, sizeof(buffer), data_size);
            break;
        }

        if (!valid_data) {
            continue;
        }

        /* Deserialize */
        LatencyTestData sample = {0};
        if (deserialize_latency_data(buffer, data_size, &sample) != 0) {
            fprintf(stderr, "[DEBUG] ERROR: Failed to deserialize latency data\n");
            continue;
        }

        /* Echo back - just send it back immediately */
        sample.echo_timestamp = get_current_time_ns();

        /* Serialize and send back */
        size_t echo_size = serialize_latency_data(&sample, echo_buffer, buffer_size);
        if (echo_size > 0) {
            Int2DdsRet write_ret = int2dds_write(g_echo_writer, echo_buffer, echo_size);
            if (write_ret == INT2DDS_RET_OK) {
                sample_count++;
            } else {
                fprintf(stderr, "[DEBUG] ERROR: int2dds_write failed with ret=%d\n", write_ret);
            }
        } else {
            fprintf(stderr, "[DEBUG] ERROR: serialize_latency_data returned 0\n");
        }

        /* Free deserialized data */
        if (sample.data) {
            free(sample.data);
        }
    }

    /* Update total samples */
    if (sample_count > 0) {
        g_stats.total_samples += sample_count;
    }
}

void on_data_available_local_latency(
    Int2DdsDataReader* reader,
    Int2DdsUserContext ctx
) {
    (void)ctx;

    if (g_should_stop) {
        return;
    }

    uint64_t receive_time_ns = get_current_time_ns();

    uint8_t buffer[65536];
    size_t data_size = 0;
    bool valid_data = false;

    /* Read all available samples */
    while (1) {
        Int2DdsRet ret = int2dds_take(reader, buffer, sizeof(buffer), &data_size, &valid_data);

        if (ret == INT2DDS_RET_NO_DATA) {
            break;
        } else if (ret != INT2DDS_RET_OK) {
            break;
        }

        if (!valid_data) {
            continue;
        }

        /* Deserialize */
        PerformanceTestData sample = {0};
        if (deserialize_performance_data(buffer, data_size, &sample) != 0) {
            continue;
        }

        /* First sample - start timing */
        if (!g_stats.first_sample_received) {
            g_stats.first_sample_received = 1;
            g_stats.start_time_ns = receive_time_ns;
            g_stats.last_report_time_ns = g_stats.start_time_ns;
            printf("First local latency sample received. Starting measurements...\n");
        }

        /* Calculate one-way latency */
        uint64_t latency_ns = receive_time_ns > sample.timestamp ?
                              (receive_time_ns - sample.timestamp) : 0;

        /* Store latency sample */
        if (g_stats.latency_count >= g_stats.latency_capacity) {
            size_t new_capacity = g_stats.latency_capacity == 0 ? 10000 : g_stats.latency_capacity * 2;
            double* new_samples = (double*)realloc(g_stats.latency_samples, new_capacity * sizeof(double));
            if (new_samples == NULL) {
                fprintf(stderr, "Failed to allocate memory for latency samples\n");
                free(sample.data);
                break;
            }
            g_stats.latency_samples = new_samples;
            g_stats.latency_capacity = new_capacity;
        }

        g_stats.latency_samples[g_stats.latency_count++] = (double)latency_ns;
        g_stats.total_samples++;
        g_stats.last_message_time_ns = receive_time_ns;

        /* Periodic progress (every 5 seconds) */
        if (g_stats.last_message_time_ns - g_stats.last_report_time_ns >= 5000000000ULL) {
            double elapsed = (g_stats.last_message_time_ns - g_stats.start_time_ns) / 1e9;

            if (g_stats.latency_count > 0) {
                double sum = 0.0;
                double min_val = g_stats.latency_samples[0];
                double max_val = g_stats.latency_samples[0];

                for (size_t i = 0; i < g_stats.latency_count; i++) {
                    double val = g_stats.latency_samples[i];
                    sum += val;
                    if (val < min_val) min_val = val;
                    if (val > max_val) max_val = val;
                }

                double avg = sum / g_stats.latency_count;

                printf("[%.1fs] Samples: %zu, Avg: %.3f ms, Min: %.3f ms, Max: %.3f ms\n",
                       elapsed, g_stats.latency_count, avg / 1e6, min_val / 1e6, max_val / 1e6);
            }

            g_stats.last_report_time_ns = g_stats.last_message_time_ns;
        }

        /* Free deserialized data */
        if (sample.data) {
            free(sample.data);
        }
    }
}

/* ====== Test Functions ====== */

int run_throughput_test(const PerfTestArgs* args) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    printf("\n=== Throughput Test (Subscriber) ===\n");
    printf("  Data size: %zu bytes\n", args->data_size);
    printf("  Reliability: %s\n", args->use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("  Warmup time: %llu seconds\n", (unsigned long long)args->warmup_time);
    printf("  Execution time: %llu seconds\n", (unsigned long long)args->execution_time);
    printf("\n");

    /* Initialize global stats */
    memset(&g_stats, 0, sizeof(g_stats));
    g_should_stop = 0;
    g_data_size = args->data_size;  /* Store data size for throughput calculation */

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
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

    /* Create topic */
    ret = int2dds_create_topic(participant, "throughput_test_topic", "PerformanceTestData", NULL, &topic);
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
        args->use_reliable ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT
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

    ret = int2dds_waitset_wait(waitset, 60000000000ULL);  /* 60 seconds timeout */
    if (ret == INT2DDS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for publisher\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    /* Warmup phase */
    if (args->warmup_time > 0) {
        printf("Starting warmup phase (%llu seconds)...\n", (unsigned long long)args->warmup_time);
        uint64_t warmup_start = get_current_time_ns();
        uint64_t warmup_end = warmup_start + (args->warmup_time * 1000000000ULL);

        while (get_current_time_ns() < warmup_end) {
            sleep_ms(1);
        }

        printf("Warmup complete. Received %llu samples during warmup. Resetting stats...\n",
               (unsigned long long)g_stats.total_samples);

        /* Reset stats for actual test */
        memset(&g_stats, 0, sizeof(g_stats));
    }

    /* Run for specified duration, monitor for timeout */
    uint64_t test_start = get_current_time_ns();
    uint64_t test_end_time = test_start + (args->execution_time * 1000000000ULL);
    uint64_t timeout_threshold = 2000000000ULL;  /* 2 seconds no data = test end */

    printf("Test running...\n");

    while (1) {
        uint64_t now = get_current_time_ns();

        /* Check execution time */
        if (now >= test_end_time) {
            printf("\nTest duration reached.\n");
            break;
        }

        /* Check for data timeout (2 seconds of no data) */
        if (g_stats.first_sample_received &&
            (now - g_stats.last_message_time_ns) > timeout_threshold) {
            printf("\nNo data received for 2 seconds. Test complete.\n");
            break;
        }

        sleep_ms(100);
    }

    /* Print final statistics */
    uint64_t end_time_ns = get_current_time_ns();
    double duration_sec = (end_time_ns - test_start) / 1e9;
    double msgs_per_sec = g_stats.total_samples / duration_sec;
    double mbps = (g_stats.bytes_received * 8.0) / (duration_sec * 1e6);
    double loss_rate = (g_stats.last_seq_num > 0) ?
                       (g_stats.lost_samples * 100.0 / g_stats.last_seq_num) : 0.0;

    /* Final progress line */
    printf("[%.1fs] Received: %llu, Avg: %.0f msg/s (%.2f Mbps), Lost: %llu, Loss rate: %.2f%%\n",
           duration_sec, (unsigned long long)g_stats.total_samples, msgs_per_sec, mbps,
           (unsigned long long)g_stats.lost_samples, loss_rate);

    printf("\n=== Performance Test Results ===\n");
    printf("Test duration: %.2f seconds\n", duration_sec);
    printf("Total samples received: %llu\n", (unsigned long long)g_stats.total_samples);
    printf("Messages per second: %.2f\n", msgs_per_sec);
    printf("Throughput: %.2f Mbps\n", mbps);
    printf("Last sequence number: %llu\n", (unsigned long long)g_stats.last_seq_num);
    printf("Lost samples: %llu\n", (unsigned long long)g_stats.lost_samples);
    printf("Loss rate: %.4f%%\n", loss_rate);

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
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsTopic* echo_topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataReaderQos* reader_qos = NULL;
    Int2DdsDataWriterQos* writer_qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    printf("\n=== Latency Test (Subscriber/Echo) ===\n");
    printf("  Data size: %zu bytes\n", args->data_size);
    printf("  Reliability: %s\n", args->use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("  Warmup time: %llu seconds\n", (unsigned long long)args->warmup_time);
    printf("  Execution time: %llu seconds\n", (unsigned long long)args->execution_time);
    printf("\n");

    /* Reset global echo writer */
    g_echo_writer = NULL;
    memset(&g_stats, 0, sizeof(g_stats));

    /* Allocate dynamic buffer for echo (header 32 bytes + payload + margin) */
    g_echo_buffer_size = (args->data_size + 64) * 2;  /* *2 for read and write buffers */
    g_echo_buffer = (uint8_t*)malloc(g_echo_buffer_size);
    if (g_echo_buffer == NULL) {
        fprintf(stderr, "Failed to allocate echo buffer (%zu bytes)\n", g_echo_buffer_size);
        return 1;
    }
    printf("Allocated echo buffer: %zu bytes\n", g_echo_buffer_size);

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
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

    /* Wait for publisher match using waitset */
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

    ret = int2dds_waitset_wait(waitset, 60000000000ULL);  /* 60 seconds */
    if (ret == INT2DDS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for publisher (60 seconds)\n");
        goto cleanup;
    } else if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    printf("Publisher connected! Echoing latency requests...\n");

    /* Warmup phase */
    if (args->warmup_time > 0) {
        printf("Starting warmup phase (%llu seconds)...\n", (unsigned long long)args->warmup_time);
        uint64_t warmup_start = get_current_time_ns();
        uint64_t warmup_end = warmup_start + (args->warmup_time * 1000000000ULL);

        while (get_current_time_ns() < warmup_end) {
            sleep_ms(1);
        }

        printf("Warmup complete. Echoed %llu samples during warmup. Resetting stats...\n",
               (unsigned long long)g_stats.total_samples);

        /* Reset stats for actual test */
        g_stats.total_samples = 0;
    }

    /* Run for specified duration */
    uint64_t start_time = get_current_time_ns();
    uint64_t test_end_time = start_time + (args->execution_time * 1000000000ULL);
    uint64_t last_report_time = start_time;
    uint64_t last_report_count = 0;

    while (1) {
        uint64_t now = get_current_time_ns();

        if (now >= test_end_time) {
            break;
        }

        /* Periodic progress (every 5 seconds) */
        if (now - last_report_time >= 5000000000ULL) {
            double elapsed = (now - start_time) / 1e9;
            double rate = g_stats.total_samples / elapsed;
            printf("[%.1fs] Echoes sent: %llu, Rate: %.0f echo/s\n",
                   elapsed, (unsigned long long)g_stats.total_samples, rate);
            last_report_time = now;
        }

        sleep_ms(100);
    }

    uint64_t end_time = get_current_time_ns();

    /* Print final statistics */
    double duration = (end_time - start_time) / 1e9;
    double rate = g_stats.total_samples / duration;

    /* Final progress line */
    printf("[%.1fs] Echoes sent: %llu, Rate: %.0f echo/s\n",
           duration, (unsigned long long)g_stats.total_samples, rate);

    printf("\n=== Latency Echo Test Results ===\n");
    printf("Duration: %.2f seconds\n", duration);
    printf("Total echoes sent: %llu\n", (unsigned long long)g_stats.total_samples);
    printf("Average echo rate: %.0f echo/s\n", rate);
    printf("Echo service completed successfully\n");

cleanup:
    g_echo_writer = NULL;

    /* Free echo buffer */
    if (g_echo_buffer) {
        free(g_echo_buffer);
        g_echo_buffer = NULL;
        g_echo_buffer_size = 0;
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
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    printf("\n=== Local Latency Test (Subscriber) ===\n");
    printf("  Data size: %zu bytes\n", args->data_size);
    printf("  Reliability: %s\n", args->use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("  Warmup time: %llu seconds\n", (unsigned long long)args->warmup_time);
    printf("  Execution time: %llu seconds\n", (unsigned long long)args->execution_time);
    printf("\n");

    /* Initialize global stats */
    memset(&g_stats, 0, sizeof(g_stats));
    g_should_stop = 0;
    g_data_size = args->data_size;

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
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

    /* Create topic */
    ret = int2dds_create_topic(participant, "local_latency_test_topic", "PerformanceTestData", NULL, &topic);
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
        args->use_reliable ? INT2DDS_QOS_RELIABILITY_RELIABLE : INT2DDS_QOS_RELIABILITY_BEST_EFFORT
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

    /* Warmup phase */
    if (args->warmup_time > 0) {
        printf("Starting warmup phase (%llu seconds)...\n", (unsigned long long)args->warmup_time);
        uint64_t warmup_start = get_current_time_ns();
        uint64_t warmup_end = warmup_start + (args->warmup_time * 1000000000ULL);

        while (get_current_time_ns() < warmup_end) {
            sleep_ms(1);
        }

        printf("Warmup complete. Received %llu samples during warmup. Resetting stats...\n",
               (unsigned long long)g_stats.total_samples);

        /* Reset stats for actual test */
        if (g_stats.latency_samples) {
            free(g_stats.latency_samples);
        }
        memset(&g_stats, 0, sizeof(g_stats));
    }

    /* Run for specified duration, monitor for timeout */
    uint64_t test_start = get_current_time_ns();
    uint64_t test_end_time = test_start + (args->execution_time * 1000000000ULL);
    uint64_t timeout_threshold = 2000000000ULL;  /* 2 seconds */

    printf("Test running...\n");

    while (1) {
        uint64_t now = get_current_time_ns();

        /* Check execution time */
        if (now >= test_end_time) {
            printf("\nTest duration reached.\n");
            break;
        }

        /* Check for data timeout */
        if (g_stats.first_sample_received &&
            (now - g_stats.last_message_time_ns) > timeout_threshold) {
            printf("\n2 second timeout after last message. Stopping...\n");
            break;
        }

        sleep_ms(100);
    }

    /* Print final statistics */
    uint64_t end_time_ns = get_current_time_ns();
    double duration_sec = (end_time_ns - test_start) / 1e9;

    /* Final progress line */
    if (g_stats.latency_count > 0) {
        double sum = 0.0;
        double min_val = g_stats.latency_samples[0];
        double max_val = g_stats.latency_samples[0];

        for (size_t i = 0; i < g_stats.latency_count; i++) {
            double val = g_stats.latency_samples[i];
            sum += val;
            if (val < min_val) min_val = val;
            if (val > max_val) max_val = val;
        }

        double avg = sum / g_stats.latency_count;

        printf("[%.1fs] Samples: %zu, Avg: %.3f ms, Min: %.3f ms, Max: %.3f ms\n",
               duration_sec, g_stats.latency_count, avg / 1e6, min_val / 1e6, max_val / 1e6);
    } else {
        printf("[%.1fs] Samples: %zu\n", duration_sec, g_stats.latency_count);
    }

    printf("\n=== Local Latency Test Results ===\n");
    printf("Test duration: %.2f seconds\n", duration_sec);
    printf("Total samples: %llu\n", (unsigned long long)g_stats.total_samples);

    if (g_stats.latency_count > 0) {
        double sum = 0.0;
        double min_val = g_stats.latency_samples[0];
        double max_val = g_stats.latency_samples[0];

        for (size_t i = 0; i < g_stats.latency_count; i++) {
            double val = g_stats.latency_samples[i];
            sum += val;
            if (val < min_val) min_val = val;
            if (val > max_val) max_val = val;
        }

        double avg = sum / g_stats.latency_count;

        printf("Latency samples: %zu\n", g_stats.latency_count);
        printf("Average latency: %.3f ms\n", avg / 1e6);
        printf("Min latency: %.3f ms\n", min_val / 1e6);
        printf("Max latency: %.3f ms\n", max_val / 1e6);
    }

cleanup:
    if (g_stats.latency_samples) {
        free(g_stats.latency_samples);
        g_stats.latency_samples = NULL;
    }

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

    printf("int2dds Performance Test Subscriber\n");
    printf("====================================\n");

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

    return result;
}
