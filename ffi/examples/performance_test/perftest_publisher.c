/**
 * int2dds FFI Performance Test Publisher
 *
 * This example demonstrates performance testing with int2dds FFI.
 * Supports three test modes:
 * - throughput: High-speed data publishing for throughput measurement
 * - latency: Round-trip latency measurement with echo
 * - local_latency: One-way latency measurement using timestamps
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <math.h>
#include <time.h>

#ifdef _WIN32
#include <windows.h>
#define sleep_ms(ms) Sleep(ms)

static uint64_t get_current_time_ns(void) {
    LARGE_INTEGER freq, counter;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&counter);
    return (uint64_t)((counter.QuadPart * 1000000000ULL) / freq.QuadPart);
}

/* High-precision sleep using busy-wait for Windows */
static void sleep_until_ns(uint64_t target_time_ns) {
    /* First, sleep coarsely if we have a lot of time */
    uint64_t now = get_current_time_ns();
    if (target_time_ns > now + 2000000) { /* More than 2ms */
        Sleep((DWORD)((target_time_ns - now - 1000000) / 1000000)); /* Sleep until 1ms before */
    }
    /* Busy-wait for precise timing */
    while (get_current_time_ns() < target_time_ns) {
        /* Spin */
    }
}

/* Short busy-wait for polling */
static void sleep_us(unsigned int us) {
    if (us == 0) return;
    uint64_t target = get_current_time_ns() + (uint64_t)us * 1000ULL;
    while (get_current_time_ns() < target) {
        /* Spin */
    }
}
#else
#include <unistd.h>
#include <sys/time.h>
#define sleep_ms(ms) usleep((ms) * 1000)

static uint64_t get_current_time_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/* High-precision sleep for Linux/Unix */
static void sleep_until_ns(uint64_t target_time_ns) {
    uint64_t now = get_current_time_ns();
    if (target_time_ns > now) {
        uint64_t sleep_ns = target_time_ns - now;
        if (sleep_ns > 1000) {
            usleep((useconds_t)(sleep_ns / 1000));
        }
    }
}

/* Short sleep for polling */
static void sleep_us(unsigned int us) {
    if (us > 0) usleep(us);
}
#endif

#include "int2dds-ffi.h"

/* ==================== Data Structures ==================== */

/* Performance data structure (for throughput and local_latency tests) */
typedef struct {
    uint64_t seq_num;
    uint64_t timestamp;
    size_t data_len;
    uint8_t* data;
} PerformanceData;

/* Latency test data structure */
typedef struct {
    uint64_t seq_num;
    uint64_t send_timestamp;
    uint64_t echo_timestamp;
    size_t data_len;
    uint8_t* data;
} LatencyTestData;

/* Performance statistics */
typedef struct {
    uint64_t* latency_samples;
    size_t latency_count;
    size_t latency_capacity;
    uint64_t total_samples;
    uint64_t lost_samples;
    uint64_t start_time_ns;
    uint64_t end_time_ns;
} PerformanceStats;

/* ==================== Serialization Functions ==================== */

/* Serialize PerformanceData to raw bytes */
/* Format: seq_num (8 bytes) + timestamp (8 bytes) + data (variable) */
static size_t serialize_performance_data(const PerformanceData* data, uint8_t* buffer, size_t buffer_size) {
    size_t total_size = 16 + data->data_len;
    if (buffer_size < total_size) {
        return 0;
    }

    /* Write seq_num (little-endian) */
    for (int i = 0; i < 8; i++) {
        buffer[i] = (uint8_t)((data->seq_num >> (i * 8)) & 0xFF);
    }

    /* Write timestamp (little-endian) */
    for (int i = 0; i < 8; i++) {
        buffer[8 + i] = (uint8_t)((data->timestamp >> (i * 8)) & 0xFF);
    }

    /* Write payload */
    if (data->data_len > 0 && data->data != NULL) {
        memcpy(buffer + 16, data->data, data->data_len);
    }

    return total_size;
}

/* Serialize LatencyTestData to raw bytes */
/* Format: seq_num (8 bytes) + send_timestamp (8 bytes) + echo_timestamp (8 bytes) + data (variable) */
static size_t serialize_latency_data(const LatencyTestData* data, uint8_t* buffer, size_t buffer_size) {
    size_t total_size = 24 + data->data_len;
    if (buffer_size < total_size) {
        return 0;
    }

    /* Write seq_num (little-endian) */
    for (int i = 0; i < 8; i++) {
        buffer[i] = (uint8_t)((data->seq_num >> (i * 8)) & 0xFF);
    }

    /* Write send_timestamp (little-endian) */
    for (int i = 0; i < 8; i++) {
        buffer[8 + i] = (uint8_t)((data->send_timestamp >> (i * 8)) & 0xFF);
    }

    /* Write echo_timestamp (little-endian) */
    for (int i = 0; i < 8; i++) {
        buffer[16 + i] = (uint8_t)((data->echo_timestamp >> (i * 8)) & 0xFF);
    }

    /* Write payload */
    if (data->data_len > 0 && data->data != NULL) {
        memcpy(buffer + 24, data->data, data->data_len);
    }

    return total_size;
}

/* Deserialize raw bytes to LatencyTestData */
static int deserialize_latency_data(const uint8_t* buffer, size_t size, LatencyTestData* data) {
    if (size < 24) {
        return -1;
    }

    /* Read seq_num (little-endian) */
    data->seq_num = 0;
    for (int i = 0; i < 8; i++) {
        data->seq_num |= ((uint64_t)buffer[i] << (i * 8));
    }

    /* Read send_timestamp (little-endian) */
    data->send_timestamp = 0;
    for (int i = 0; i < 8; i++) {
        data->send_timestamp |= ((uint64_t)buffer[8 + i] << (i * 8));
    }

    /* Read echo_timestamp (little-endian) */
    data->echo_timestamp = 0;
    for (int i = 0; i < 8; i++) {
        data->echo_timestamp |= ((uint64_t)buffer[16 + i] << (i * 8));
    }

    data->data_len = size - 24;
    return 0;
}

/* ==================== Statistics Functions ==================== */

static void stats_init(PerformanceStats* stats, size_t initial_capacity) {
    stats->latency_samples = NULL;
    stats->latency_count = 0;
    stats->latency_capacity = 0;
    stats->total_samples = 0;
    stats->lost_samples = 0;
    stats->start_time_ns = 0;
    stats->end_time_ns = 0;

    if (initial_capacity > 0) {
        stats->latency_samples = (uint64_t*)malloc(initial_capacity * sizeof(uint64_t));
        if (stats->latency_samples) {
            stats->latency_capacity = initial_capacity;
        }
    }
}

static void stats_free(PerformanceStats* stats) {
    if (stats->latency_samples) {
        free(stats->latency_samples);
        stats->latency_samples = NULL;
    }
    stats->latency_capacity = 0;
    stats->latency_count = 0;
}

static void stats_add_latency(PerformanceStats* stats, uint64_t latency_ns) {
    if (stats->latency_count >= stats->latency_capacity) {
        size_t new_capacity = stats->latency_capacity == 0 ? 1024 : stats->latency_capacity * 2;
        uint64_t* new_samples = (uint64_t*)realloc(stats->latency_samples, new_capacity * sizeof(uint64_t));
        if (!new_samples) return;
        stats->latency_samples = new_samples;
        stats->latency_capacity = new_capacity;
    }
    stats->latency_samples[stats->latency_count++] = latency_ns;
}

static int compare_uint64(const void* a, const void* b) {
    uint64_t va = *(const uint64_t*)a;
    uint64_t vb = *(const uint64_t*)b;
    if (va < vb) return -1;
    if (va > vb) return 1;
    return 0;
}

static void stats_calculate_latency(PerformanceStats* stats, double* avg, double* min_val, double* max_val) {
    if (stats->latency_count == 0) {
        *avg = *min_val = *max_val = 0.0;
        return;
    }

    qsort(stats->latency_samples, stats->latency_count, sizeof(uint64_t), compare_uint64);

    double sum = 0.0;
    for (size_t i = 0; i < stats->latency_count; i++) {
        sum += (double)stats->latency_samples[i];
    }

    *avg = sum / stats->latency_count;
    *min_val = (double)stats->latency_samples[0];
    *max_val = (double)stats->latency_samples[stats->latency_count - 1];
}

static void stats_calculate_current_latency(PerformanceStats* stats, double* avg, double* min_val, double* max_val) {
    if (stats->latency_count == 0) {
        *avg = *min_val = *max_val = 0.0;
        return;
    }

    double sum = 0.0;
    *min_val = (double)stats->latency_samples[0];
    *max_val = (double)stats->latency_samples[0];

    for (size_t i = 0; i < stats->latency_count; i++) {
        double val = (double)stats->latency_samples[i];
        sum += val;
        if (val < *min_val) *min_val = val;
        if (val > *max_val) *max_val = val;
    }

    *avg = sum / stats->latency_count;
}

static void stats_calculate_throughput(PerformanceStats* stats, size_t data_size, double* msgs_per_sec, double* mbps) {
    double duration_sec = (double)(stats->end_time_ns - stats->start_time_ns) / 1000000000.0;
    if (duration_sec <= 0) {
        *msgs_per_sec = 0.0;
        *mbps = 0.0;
        return;
    }

    *msgs_per_sec = (double)stats->total_samples / duration_sec;
    *mbps = ((double)stats->total_samples * (double)data_size * 8.0) / (duration_sec * 1000000.0);
}

static void stats_print_summary(PerformanceStats* stats, bool show_latency, size_t data_size) {
    double msgs_per_sec, mbps;
    stats_calculate_throughput(stats, data_size, &msgs_per_sec, &mbps);

    printf("\n=== Performance Summary ===\n");
    printf("Total samples: %llu\n", (unsigned long long)stats->total_samples);
    printf("Lost samples: %llu\n", (unsigned long long)stats->lost_samples);
    printf("Rate: %.2f msg/s\n", msgs_per_sec);
    printf("Throughput: %.2f Mbps\n", mbps);

    if (show_latency && stats->latency_count > 0) {
        double avg, min_val, max_val;
        stats_calculate_latency(stats, &avg, &min_val, &max_val);
        printf("Average latency: %.2f ms\n", avg / 1000000.0);
        printf("Min latency: %.2f ms\n", min_val / 1000000.0);
        printf("Max latency: %.2f ms\n", max_val / 1000000.0);
    }
}

/* ==================== CSV Export Functions ==================== */

static void save_throughput_csv(const char* prefix, size_t data_len, double hz,
                                uint64_t total_sent, double msgs_per_sec, double mbps) {
    char filename[256];
    snprintf(filename, sizeof(filename), "%s_throughput_results_%zub_%.0fhz.csv",
             prefix, data_len, hz);

    FILE* file = fopen(filename, "r");
    bool file_exists = (file != NULL);
    if (file) fclose(file);

    file = fopen(filename, "a");
    if (!file) {
        fprintf(stderr, "Failed to open CSV file: %s\n", filename);
        return;
    }

    if (!file_exists) {
        fprintf(file, "Test_Cycle,Timestamp,Total_Sent_Messages,Messages_Per_Second,Throughput_Mbps\n");
    }

    uint64_t timestamp_ms = (uint64_t)time(NULL) * 1000;
    fprintf(file, "1,%llu,%llu,%.0f,%.2f\n",
            (unsigned long long)timestamp_ms,
            (unsigned long long)total_sent,
            msgs_per_sec, mbps);

    fclose(file);
    printf("Results saved to: %s\n", filename);
}

static void save_latency_csv(const char* prefix, size_t data_len, double hz,
                             double avg_ms, double min_ms, double max_ms) {
    char filename[256];
    snprintf(filename, sizeof(filename), "%s_latency_results_%zub_%.0fhz.csv",
             prefix, data_len, hz);

    FILE* file = fopen(filename, "r");
    bool file_exists = (file != NULL);
    if (file) fclose(file);

    file = fopen(filename, "a");
    if (!file) {
        fprintf(stderr, "Failed to open CSV file: %s\n", filename);
        return;
    }

    if (!file_exists) {
        fprintf(file, "Timestamp,Average_Latency_ms,Min_Latency_ms,Max_Latency_ms\n");
    }

    uint64_t timestamp_ms = (uint64_t)time(NULL) * 1000;
    fprintf(file, "%llu,%.2f,%.2f,%.2f\n",
            (unsigned long long)timestamp_ms, avg_ms, min_ms, max_ms);

    fclose(file);
    printf("Results saved to: %s\n", filename);
}

/* ==================== Throughput Test ==================== */

static void run_throughput_test(int32_t domain_id, size_t data_len, int execution_time,
                                const char* reliability, double hz) {
    printf("Starting throughput test:\n");
    printf("  Data size: %zu bytes\n", data_len);
    printf("  Reliability: %s\n", reliability);
    printf("  Execution time: %d seconds\n", execution_time);
    if (hz > 0) {
        printf("  Rate limit: %.0f Hz\n", hz);
    }

    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "throughput_publisher", domain_id, &participant);
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

    /* Create DataWriter QoS */
    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    /* Set reliability */
    if (strcmp(reliability, "reliable") == 0) {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 0);
    }
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

    /* Create DataWriter */
    ret = int2dds_create_datawriter(publisher, topic, qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for subscriber to connect...\n");

    /* Create WaitSet and attach writer for publication matched */
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

    /* Wait for subscriber using publication matched */
    while (1) {
        int32_t total_count = 0, current_count = 0;
        ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
        if (ret == INT2DDS_RET_OK && current_count > 0) {
            printf("Subscriber connected. Starting throughput test...\n");
            break;
        }
        sleep_ms(100);
    }

    /* Allocate buffers */
    size_t buffer_size = 16 + data_len;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    uint8_t* payload = (uint8_t*)malloc(data_len);
    if (!buffer || !payload) {
        fprintf(stderr, "Failed to allocate buffers\n");
        goto cleanup;
    }
    memset(payload, 0xAA, data_len);

    /* Test variables */
    uint64_t start_time = get_current_time_ns();
    uint64_t end_time = start_time + (uint64_t)execution_time * 1000000000ULL;
    uint64_t seq_num = 0;
    uint64_t total_sent = 0;
    uint64_t last_report_time = start_time;
    uint64_t last_report_count = 0;

    /* Hz-based timing */
    uint64_t send_interval_ns = (hz > 0) ? (uint64_t)(1000000000.0 / hz) : 0;
    uint64_t next_send_time = start_time + send_interval_ns;

    while (get_current_time_ns() < end_time) {
        /* Hz-based rate limiting - wait until next send time */
        if (hz > 0) {
            sleep_until_ns(next_send_time);
        }

        /* Create and serialize data */
        PerformanceData data;
        data.seq_num = seq_num;
        data.timestamp = get_current_time_ns();
        data.data_len = data_len;
        data.data = payload;

        size_t serialized_size = serialize_performance_data(&data, buffer, buffer_size);
        if (serialized_size == 0) {
            fprintf(stderr, "Failed to serialize data\n");
            continue;
        }

        /* Write data */
        uint64_t write_start = get_current_time_ns();
        ret = int2dds_write(writer, buffer, serialized_size);
        uint64_t write_duration = get_current_time_ns() - write_start;

        if (write_duration >= 1000000000ULL) {
            printf("[WARNING] write() took %.3f s (seq_num: %llu)\n",
                   write_duration / 1000000000.0, (unsigned long long)seq_num);
        }

        seq_num++;
        total_sent++;

        /* Update next send time for Hz limiting */
        if (hz > 0) {
            next_send_time += send_interval_ns;
        }

        /* Periodic report every 5 seconds */
        uint64_t now = get_current_time_ns();
        if (now - last_report_time >= 5000000000ULL) {
            double elapsed = (double)(now - start_time) / 1000000000.0;
            double interval_elapsed = (double)(now - last_report_time) / 1000000000.0;
            uint64_t interval_sent = total_sent - last_report_count;

            double avg_rate = total_sent / elapsed;
            double interval_rate = interval_sent / interval_elapsed;
            double avg_mbps = (total_sent * data_len * 8.0) / (elapsed * 1000000.0);
            double interval_mbps = (interval_sent * data_len * 8.0) / (interval_elapsed * 1000000.0);

            printf("[%.1fs] Sent: %llu, Avg: %.0f msg/s (%.2f Mbps), Interval: %.0f msg/s (%.2f Mbps)\n",
                   elapsed, (unsigned long long)total_sent,
                   avg_rate, avg_mbps, interval_rate, interval_mbps);

            last_report_time = now;
            last_report_count = total_sent;
        }
    }

    /* Final statistics */
    double elapsed = (double)(get_current_time_ns() - start_time) / 1000000000.0;
    double final_rate = total_sent / elapsed;
    double mbps = (total_sent * data_len * 8.0) / (elapsed * 1000000.0);

    printf("\n=== Throughput Test Publisher Results ===\n");
    printf("Total samples sent: %llu\n", (unsigned long long)total_sent);
    printf("Test duration: %.2f seconds\n", elapsed);
    printf("Send rate: %.2f msg/s\n", final_rate);
    printf("Throughput: %.2f Mbps\n", mbps);

    /* Save to CSV */
    save_throughput_csv("publisher", data_len, hz, total_sent, final_rate, mbps);

    free(buffer);
    free(payload);

cleanup:
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ==================== Latency Test ==================== */

static void run_latency_test(int32_t domain_id, size_t data_len, int execution_time,
                             const char* reliability, int latency_interval_ms,
                             int latency_count, double hz) {
    printf("Starting latency test:\n");
    printf("  Data size: %zu bytes\n", data_len);
    printf("  Reliability: %s\n", reliability);
    if (latency_count > 0) {
        printf("  Latency count: %d samples\n", latency_count);
    } else {
        printf("  Execution time: %d seconds\n", execution_time);
    }
    if (hz > 0) {
        printf("  Rate limit: %.0f Hz\n", hz);
    } else {
        printf("  Latency interval: %d ms\n", latency_interval_ms);
    }

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

    PerformanceStats stats;
    stats_init(&stats, 10000);

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "latency_publisher", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create publisher and subscriber */
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

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

    /* Create DataWriter QoS */
    ret = int2dds_datawriter_qos_create_default(&writer_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create writer QoS: %d\n", ret);
        goto cleanup;
    }

    if (strcmp(reliability, "reliable") == 0) {
        ret = int2dds_datawriter_qos_set_reliability(writer_qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datawriter_qos_set_reliability(writer_qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 0);
    }
    int2dds_datawriter_qos_set_history(writer_qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);

    /* Create DataReader QoS */
    ret = int2dds_datareader_qos_create_default(&reader_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create reader QoS: %d\n", ret);
        goto cleanup;
    }

    if (strcmp(reliability, "reliable") == 0) {
        ret = int2dds_datareader_qos_set_reliability(reader_qos, INT2DDS_QOS_RELIABILITY_RELIABLE);
    } else {
        ret = int2dds_datareader_qos_set_reliability(reader_qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT);
    }
    int2dds_datareader_qos_set_history(reader_qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);

    /* Create DataWriter and DataReader */
    ret = int2dds_create_datawriter(publisher, topic, writer_qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datareader(subscriber, echo_topic, reader_qos, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for subscriber to connect...\n");

    /* Wait for subscriber */
    while (1) {
        int32_t total_count = 0, current_count = 0;
        ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
        if (ret == INT2DDS_RET_OK && current_count > 0) {
            printf("Subscriber connected. Starting latency test...\n");
            break;
        }
        sleep_ms(100);
    }

    /* Allocate buffers */
    size_t send_buffer_size = 24 + data_len;
    size_t recv_buffer_size = 24 + data_len;
    uint8_t* send_buffer = (uint8_t*)malloc(send_buffer_size);
    uint8_t* recv_buffer = (uint8_t*)malloc(recv_buffer_size);
    uint8_t* payload = (uint8_t*)malloc(data_len);
    if (!send_buffer || !recv_buffer || !payload) {
        fprintf(stderr, "Failed to allocate buffers\n");
        goto cleanup;
    }
    memset(payload, 0xAA, data_len);

    /* Test variables */
    uint64_t start_time = get_current_time_ns();
    uint64_t end_time = start_time + (uint64_t)execution_time * 1000000000ULL;
    uint64_t seq_num = 0;
    uint64_t send_interval_ns = (hz > 0) ? (uint64_t)(1000000000.0 / hz) : 0;
    uint64_t next_send_time = start_time + send_interval_ns;
    uint64_t last_report_time = start_time;
    uint64_t last_report_count = 0;

    stats.start_time_ns = start_time;

    bool running = true;
    while (running) {
        /* Check termination conditions */
        if (latency_count > 0) {
            if (seq_num >= (uint64_t)latency_count || get_current_time_ns() >= end_time) {
                if (seq_num < (uint64_t)latency_count) {
                    printf("Test stopped by execution_time limit. Sent %llu / %d samples.\n",
                           (unsigned long long)seq_num, latency_count);
                }
                break;
            }
        } else {
            if (get_current_time_ns() >= end_time) {
                break;
            }
        }

        /* Create and send data */
        LatencyTestData send_data;
        send_data.seq_num = seq_num;
        send_data.send_timestamp = get_current_time_ns();
        send_data.echo_timestamp = 0;
        send_data.data_len = data_len;
        send_data.data = payload;

        size_t serialized_size = serialize_latency_data(&send_data, send_buffer, send_buffer_size);
        ret = int2dds_write(writer, send_buffer, serialized_size);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to write: %d\n", ret);
        }

        seq_num++;

        /* Wait for echo response with timeout */
        uint64_t echo_timeout = get_current_time_ns() + 1000000000ULL; /* 1 second timeout */
        while (get_current_time_ns() < echo_timeout) {
            size_t data_size = 0;
            bool valid_data = false;
            ret = int2dds_take(reader, recv_buffer, recv_buffer_size, &data_size, &valid_data);

            if (ret == INT2DDS_RET_OK && valid_data) {
                uint64_t receive_time = get_current_time_ns();
                LatencyTestData recv_data;
                if (deserialize_latency_data(recv_buffer, data_size, &recv_data) == 0) {
                    uint64_t round_trip_ns = receive_time - recv_data.send_timestamp;
                    uint64_t latency_ns = round_trip_ns / 2;
                    stats_add_latency(&stats, latency_ns);
                    stats.total_samples++;
                }
                break;
            } else if (ret == INT2DDS_RET_NO_DATA) {
                sleep_us(100);
            }
        }

        /* Periodic report every 5 seconds */
        uint64_t now = get_current_time_ns();
        if (now - last_report_time >= 5000000000ULL) {
            double elapsed = (double)(now - start_time) / 1000000000.0;
            double interval_elapsed = (double)(now - last_report_time) / 1000000000.0;
            uint64_t interval_sent = seq_num - last_report_count;

            double avg_rate = seq_num / elapsed;
            double interval_rate = interval_sent / interval_elapsed;

            double avg_latency, min_latency, max_latency;
            stats_calculate_current_latency(&stats, &avg_latency, &min_latency, &max_latency);

            if (latency_count > 0) {
                printf("[%.1fs] Sent: %llu / %d, Avg: %.0f msg/s, Interval: %.0f msg/s, "
                       "Latency: %.2fms (min: %.2fms, max: %.2fms)\n",
                       elapsed, (unsigned long long)seq_num, latency_count,
                       avg_rate, interval_rate,
                       avg_latency / 1000000.0, min_latency / 1000000.0, max_latency / 1000000.0);
            } else {
                printf("[%.1fs] Sent: %llu, Avg: %.0f msg/s, Interval: %.0f msg/s, "
                       "Latency: %.2fms (min: %.2fms, max: %.2fms)\n",
                       elapsed, (unsigned long long)seq_num,
                       avg_rate, interval_rate,
                       avg_latency / 1000000.0, min_latency / 1000000.0, max_latency / 1000000.0);
            }

            last_report_time = now;
            last_report_count = seq_num;
        }

        /* Rate limiting */
        if (hz > 0) {
            next_send_time += send_interval_ns;
            sleep_until_ns(next_send_time);
        } else {
            sleep_ms(latency_interval_ms);
        }
    }

    sleep_ms(1000);

    stats.end_time_ns = get_current_time_ns();
    stats_print_summary(&stats, true, data_len);

    /* Save to CSV if we have samples */
    if (stats.latency_count > 0) {
        double avg, min_val, max_val;
        stats_calculate_latency(&stats, &avg, &min_val, &max_val);
        save_latency_csv("publisher", data_len, hz,
                         avg / 1000000.0, min_val / 1000000.0, max_val / 1000000.0);
    } else {
        printf("\nNo latency samples collected. Skipping CSV export.\n");
    }

    free(send_buffer);
    free(recv_buffer);
    free(payload);

cleanup:
    stats_free(&stats);
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ==================== Local Latency Test ==================== */

static void run_local_latency_test(int32_t domain_id, size_t data_len, int execution_time,
                                   const char* reliability, double hz) {
    printf("Starting local latency test (publisher):\n");
    printf("  Data size: %zu bytes\n", data_len);
    printf("  Reliability: %s\n", reliability);
    printf("  Execution time: %d seconds\n", execution_time);
    if (hz > 0) {
        printf("  Rate limit: %.0f Hz\n", hz);
    } else {
        printf("  Rate: unlimited\n");
    }

    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "local_latency_publisher", domain_id, &participant);
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

    /* Create DataWriter QoS */
    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    if (strcmp(reliability, "reliable") == 0) {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 0);
    }
    int2dds_datawriter_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);

    /* Create DataWriter */
    ret = int2dds_create_datawriter(publisher, topic, qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for subscriber to connect...\n");

    /* Wait for subscriber */
    while (1) {
        int32_t total_count = 0, current_count = 0;
        ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
        if (ret == INT2DDS_RET_OK && current_count > 0) {
            printf("Subscriber connected. Starting local latency test...\n");
            break;
        }
        sleep_ms(100);
    }

    /* Allocate buffers */
    size_t buffer_size = 16 + data_len;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    uint8_t* payload = (uint8_t*)malloc(data_len);
    if (!buffer || !payload) {
        fprintf(stderr, "Failed to allocate buffers\n");
        goto cleanup;
    }
    memset(payload, 0xAA, data_len);

    /* Test variables */
    uint64_t start_time = get_current_time_ns();
    uint64_t end_time = start_time + (uint64_t)execution_time * 1000000000ULL;
    uint64_t seq_num = 0;
    uint64_t total_sent = 0;
    uint64_t last_report_time = start_time;
    uint64_t last_report_count = 0;

    /* Hz-based timing */
    uint64_t send_interval_ns = (hz > 0) ? (uint64_t)(1000000000.0 / hz) : 0;
    uint64_t next_send_time = start_time + send_interval_ns;

    while (get_current_time_ns() < end_time) {
        /* Hz-based rate limiting - wait until next send time */
        if (hz > 0) {
            sleep_until_ns(next_send_time);
        }

        /* Create and serialize data */
        PerformanceData data;
        data.seq_num = seq_num;
        data.timestamp = get_current_time_ns();
        data.data_len = data_len;
        data.data = payload;

        size_t serialized_size = serialize_performance_data(&data, buffer, buffer_size);
        ret = int2dds_write(writer, buffer, serialized_size);

        seq_num++;
        total_sent++;

        /* Update next send time for Hz limiting */
        if (hz > 0) {
            next_send_time += send_interval_ns;
        }

        /* Periodic report */
        uint64_t now = get_current_time_ns();
        if (now - last_report_time >= 5000000000ULL) {
            double elapsed = (double)(now - start_time) / 1000000000.0;
            double interval_elapsed = (double)(now - last_report_time) / 1000000000.0;
            uint64_t interval_sent = total_sent - last_report_count;

            double avg_rate = total_sent / elapsed;
            double interval_rate = interval_sent / interval_elapsed;

            printf("[%.1fs] Sent: %llu, Avg: %.0f msg/s, Interval: %.0f msg/s\n",
                   elapsed, (unsigned long long)total_sent, avg_rate, interval_rate);

            last_report_time = now;
            last_report_count = total_sent;
        }
    }

    /* Final statistics */
    double elapsed = (double)(get_current_time_ns() - start_time) / 1000000000.0;
    double final_rate = total_sent / elapsed;

    printf("\n=== Local Latency Test Publisher Results ===\n");
    printf("Total samples sent: %llu\n", (unsigned long long)total_sent);
    printf("Test duration: %.2f seconds\n", elapsed);
    printf("Send rate: %.2f msg/s\n", final_rate);

    /* Save to CSV */
    char filename[256];
    if (hz > 0) {
        snprintf(filename, sizeof(filename), "publisher_local_latency_results_%zub_%.0fhz.csv",
                 data_len, hz);
    } else {
        snprintf(filename, sizeof(filename), "publisher_local_latency_results_%zub.csv", data_len);
    }

    FILE* file = fopen(filename, "r");
    bool file_exists = (file != NULL);
    if (file) fclose(file);

    file = fopen(filename, "a");
    if (file) {
        if (!file_exists) {
            fprintf(file, "Timestamp,Total_Sent_Messages,Duration_Seconds,Messages_Per_Second\n");
        }
        uint64_t timestamp_ms = (uint64_t)time(NULL) * 1000;
        fprintf(file, "%llu,%llu,%.2f,%.2f\n",
                (unsigned long long)timestamp_ms,
                (unsigned long long)total_sent, elapsed, final_rate);
        fclose(file);
        printf("Results saved to: %s\n", filename);
    }

    free(buffer);
    free(payload);

cleanup:
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ==================== Main ==================== */

static void print_help(const char* program_name) {
    printf("Usage: %s [options]\n", program_name);
    printf("Options:\n");
    printf("  --mode, -m <mode>        Test mode: throughput, latency, local_latency (default: throughput)\n");
    printf("  --domain-id, -d <id>     Domain ID (default: 0)\n");
    printf("  --data-size, -s <size>   Data size in bytes (default: 1024)\n");
    printf("  --time, -t <seconds>     Execution time (default: 30)\n");
    printf("  --reliability, -r <mode> Reliability: best_effort, reliable (default: best_effort)\n");
    printf("  --hz, -z <rate>          Rate limit in Hz (default: unlimited)\n");
    printf("  --interval, -i <ms>      Latency test interval in ms (default: 1000)\n");
    printf("  --count, -c <num>        Number of latency samples (default: 0 = time-based)\n");
    printf("  --help, -h               Show this help\n");
}

int main(int argc, char* argv[]) {
    /* Default parameters */
    char test_mode[64] = "throughput";
    int32_t domain_id = 17;
    size_t data_len = 1024;
    int execution_time = 30;
    char reliability[64] = "best_effort";
    double hz = 0.0;
    int latency_interval_ms = 1000;
    int latency_count = 0;

    /* Parse command line arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--mode") == 0 || strcmp(argv[i], "-m") == 0) {
            if (i + 1 < argc) {
                strncpy(test_mode, argv[++i], sizeof(test_mode) - 1);
                test_mode[sizeof(test_mode) - 1] = '\0';
            }
        } else if (strcmp(argv[i], "--domain-id") == 0 || strcmp(argv[i], "-d") == 0) {
            if (i + 1 < argc) {
                domain_id = atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--data-size") == 0 || strcmp(argv[i], "-s") == 0) {
            if (i + 1 < argc) {
                data_len = (size_t)atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--time") == 0 || strcmp(argv[i], "-t") == 0) {
            if (i + 1 < argc) {
                execution_time = atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--reliability") == 0 || strcmp(argv[i], "-r") == 0) {
            if (i + 1 < argc) {
                strncpy(reliability, argv[++i], sizeof(reliability) - 1);
                reliability[sizeof(reliability) - 1] = '\0';
            }
        } else if (strcmp(argv[i], "--hz") == 0 || strcmp(argv[i], "-z") == 0) {
            if (i + 1 < argc) {
                hz = atof(argv[++i]);
            }
        } else if (strcmp(argv[i], "--count") == 0 || strcmp(argv[i], "-c") == 0) {
            if (i + 1 < argc) {
                latency_count = atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--interval") == 0 || strcmp(argv[i], "-i") == 0) {
            if (i + 1 < argc) {
                latency_interval_ms = atoi(argv[++i]);
            }
        } else if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "-h") == 0) {
            print_help(argv[0]);
            return 0;
        }
    }

    printf("int2dds FFI Performance Test Publisher\n");
    printf("Test mode: %s\n", test_mode);

    if (strcmp(test_mode, "throughput") == 0 || strcmp(test_mode, "thr") == 0 || strcmp(test_mode, "1") == 0) {
        printf("Starting throughput test mode\n");
        run_throughput_test(domain_id, data_len, execution_time, reliability, hz);
    } else if (strcmp(test_mode, "latency") == 0 || strcmp(test_mode, "lat") == 0 || strcmp(test_mode, "2") == 0) {
        printf("Starting latency test mode\n");
        run_latency_test(domain_id, data_len, execution_time, reliability, latency_interval_ms, latency_count, hz);
    } else if (strcmp(test_mode, "local_latency") == 0 || strcmp(test_mode, "local") == 0 ||
               strcmp(test_mode, "ll") == 0 || strcmp(test_mode, "3") == 0) {
        printf("Starting local latency test mode\n");
        run_local_latency_test(domain_id, data_len, execution_time, reliability, hz);
    } else {
        printf("Invalid test mode '%s'. Supported modes: throughput, latency, local_latency\n", test_mode);
        printf("Defaulting to throughput test.\n");
        run_throughput_test(domain_id, data_len, execution_time, reliability, hz);
    }

    return 0;
}
