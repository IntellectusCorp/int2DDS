/**
 * int2dds FFI Performance Test Subscriber
 *
 * This example demonstrates performance testing with int2dds FFI.
 * Supports three test modes:
 * - throughput: Receive data and measure throughput/loss rate
 * - latency: Echo mode - receive data and send back for RTT measurement
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

/* Short busy-wait for polling (Windows Sleep has ~15ms minimum) */
static void sleep_us(unsigned int us) {
    if (us == 0) return;
    uint64_t target = get_current_time_ns() + (uint64_t)us * 1000ULL;
    while (get_current_time_ns() < target) {
        /* Spin for short waits */
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
} PerformanceData;

/* Latency test data structure */
typedef struct {
    uint64_t seq_num;
    uint64_t send_timestamp;
    uint64_t echo_timestamp;
    size_t data_len;
} LatencyTestData;

/* Performance statistics */
typedef struct {
    uint64_t* latency_samples;
    size_t latency_count;
    size_t latency_capacity;
    uint64_t total_samples;
    uint64_t lost_samples;
    uint64_t* seen_seqs;
    size_t seen_seqs_count;
    size_t seen_seqs_capacity;
    uint64_t expected_seq;
    uint64_t start_time_ns;
    uint64_t end_time_ns;
} PerformanceStats;

/* ==================== Deserialization Functions ==================== */

/* Deserialize raw bytes to PerformanceData */
static int deserialize_performance_data(const uint8_t* buffer, size_t size, PerformanceData* data) {
    if (size < 16) {
        return -1;
    }

    /* Read seq_num (little-endian) */
    data->seq_num = 0;
    for (int i = 0; i < 8; i++) {
        data->seq_num |= ((uint64_t)buffer[i] << (i * 8));
    }

    /* Read timestamp (little-endian) */
    data->timestamp = 0;
    for (int i = 0; i < 8; i++) {
        data->timestamp |= ((uint64_t)buffer[8 + i] << (i * 8));
    }

    data->data_len = size - 16;
    return 0;
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
    stats->seen_seqs = NULL;
    stats->seen_seqs_count = 0;
    stats->seen_seqs_capacity = 0;
    stats->expected_seq = 0;
    stats->start_time_ns = 0;
    stats->end_time_ns = 0;

    if (initial_capacity > 0) {
        stats->latency_samples = (uint64_t*)malloc(initial_capacity * sizeof(uint64_t));
        if (stats->latency_samples) {
            stats->latency_capacity = initial_capacity;
        }
        stats->seen_seqs = (uint64_t*)malloc(initial_capacity * sizeof(uint64_t));
        if (stats->seen_seqs) {
            stats->seen_seqs_capacity = initial_capacity;
        }
    }
}

static void stats_free(PerformanceStats* stats) {
    if (stats->latency_samples) {
        free(stats->latency_samples);
        stats->latency_samples = NULL;
    }
    if (stats->seen_seqs) {
        free(stats->seen_seqs);
        stats->seen_seqs = NULL;
    }
    stats->latency_capacity = 0;
    stats->latency_count = 0;
    stats->seen_seqs_capacity = 0;
    stats->seen_seqs_count = 0;
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

static bool stats_add_seen_seq(PerformanceStats* stats, uint64_t seq) {
    /* Check for duplicate */
    for (size_t i = 0; i < stats->seen_seqs_count; i++) {
        if (stats->seen_seqs[i] == seq) {
            return false; /* Duplicate */
        }
    }

    /* Add new seq */
    if (stats->seen_seqs_count >= stats->seen_seqs_capacity) {
        size_t new_capacity = stats->seen_seqs_capacity == 0 ? 1024 : stats->seen_seqs_capacity * 2;
        uint64_t* new_seqs = (uint64_t*)realloc(stats->seen_seqs, new_capacity * sizeof(uint64_t));
        if (!new_seqs) return false;
        stats->seen_seqs = new_seqs;
        stats->seen_seqs_capacity = new_capacity;
    }
    stats->seen_seqs[stats->seen_seqs_count++] = seq;
    return true;
}

static uint64_t stats_get_max_seq(PerformanceStats* stats) {
    if (stats->seen_seqs_count == 0) return 0;
    uint64_t max_seq = stats->seen_seqs[0];
    for (size_t i = 1; i < stats->seen_seqs_count; i++) {
        if (stats->seen_seqs[i] > max_seq) {
            max_seq = stats->seen_seqs[i];
        }
    }
    return max_seq;
}

static void stats_calculate_lost_from_hashset(PerformanceStats* stats) {
    if (stats->seen_seqs_count == 0) {
        stats->lost_samples = 0;
        return;
    }

    uint64_t max_seq = stats_get_max_seq(stats);
    uint64_t expected_samples = max_seq + 1;
    stats->lost_samples = expected_samples - stats->seen_seqs_count;
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

static void stats_print_summary(PerformanceStats* stats, size_t data_size) {
    double msgs_per_sec, mbps;
    stats_calculate_throughput(stats, data_size, &msgs_per_sec, &mbps);

    printf("\n=== Performance Summary ===\n");
    printf("Total samples: %llu\n", (unsigned long long)stats->total_samples);
    printf("Lost samples: %llu\n", (unsigned long long)stats->lost_samples);
    printf("Rate: %.2f msg/s\n", msgs_per_sec);
    printf("Throughput: %.2f Mbps\n", mbps);
}

/* ==================== CSV Export Functions ==================== */

static void save_throughput_csv(const char* prefix, size_t data_len, double hz,
                                uint64_t total_received, double msgs_per_sec, double mbps,
                                uint64_t lost, double loss_rate) {
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
        fprintf(file, "Timestamp,Total_Received_Messages,Messages_Per_Second,Lost_Messages,Loss_Rate_Percent,Throughput_Mbps\n");
    }

    uint64_t timestamp_ms = (uint64_t)time(NULL) * 1000;
    fprintf(file, "%llu,%llu,%.0f,%llu,%.2f,%.2f\n",
            (unsigned long long)timestamp_ms,
            (unsigned long long)total_received,
            msgs_per_sec,
            (unsigned long long)lost,
            loss_rate, mbps);

    fclose(file);
    printf("Results saved to: %s\n", filename);
}

static void save_local_latency_csv(const char* prefix, size_t data_len, double hz,
                                   uint64_t total_samples, double avg_ms, double min_ms, double max_ms) {
    char filename[256];
    if (hz > 0) {
        snprintf(filename, sizeof(filename), "%s_local_latency_results_%zub_%.0fhz.csv",
                 prefix, data_len, hz);
    } else {
        snprintf(filename, sizeof(filename), "%s_local_latency_results_%zub.csv",
                 prefix, data_len);
    }

    FILE* file = fopen(filename, "r");
    bool file_exists = (file != NULL);
    if (file) fclose(file);

    file = fopen(filename, "a");
    if (!file) {
        fprintf(stderr, "Failed to open CSV file: %s\n", filename);
        return;
    }

    if (!file_exists) {
        fprintf(file, "Timestamp,Total_Samples,Avg_Latency_ms,Min_Latency_ms,Max_Latency_ms\n");
    }

    uint64_t timestamp_ms = (uint64_t)time(NULL) * 1000;
    fprintf(file, "%llu,%llu,%.3f,%.3f,%.3f\n",
            (unsigned long long)timestamp_ms,
            (unsigned long long)total_samples,
            avg_ms, min_ms, max_ms);

    fclose(file);
    printf("Results saved to: %s\n", filename);
}

/* ==================== Throughput Test ==================== */

static void run_throughput_subscriber(int32_t domain_id, const char* reliability,
                                      int execution_time, bool out_of_order,
                                      size_t data_len, double hz) {
    printf("Starting throughput subscriber:\n");
    printf("  Reliability: %s\n", reliability);
    printf("  Execution time: %d seconds\n", execution_time);
    printf("  Out-of-order mode: %s\n", out_of_order ? "true" : "false");

    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    PerformanceStats stats;
    stats_init(&stats, 100000);

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "throughput_subscriber", domain_id, &participant);
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
    ret = int2dds_create_topic(participant, "throughput_test_topic", "PerformanceData", NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* Create DataReader QoS */
    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    if (strcmp(reliability, "reliable") == 0) {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE);
    } else {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT);
    }
    int2dds_datareader_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);

    /* Create DataReader */
    ret = int2dds_create_datareader(subscriber, topic, qos, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for publisher...\n");

    /* Wait for publisher */
    while (1) {
        int32_t total_count = 0, current_count = 0;
        ret = int2dds_get_subscription_matched_status(reader, &total_count, &current_count);
        if (ret == INT2DDS_RET_OK && current_count > 0) {
            printf("Publisher matched! Starting measurements...\n");
            break;
        }
        sleep_ms(100);
    }

    /* Allocate buffer */
    size_t buffer_size = 16 + data_len + 1024; /* Extra space for safety */
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (!buffer) {
        fprintf(stderr, "Failed to allocate buffer\n");
        goto cleanup;
    }

    /* Test variables */
    uint64_t last_message_time = 0;
    bool first_sample = true;
    uint64_t last_report_time = 0;
    int last_report_interval = 0;

    /* Main receive loop */
    while (1) {
        size_t data_size = 0;
        bool valid_data = false;
        ret = int2dds_take(reader, buffer, buffer_size, &data_size, &valid_data);

        if (ret == INT2DDS_RET_OK && valid_data) {
            uint64_t now = get_current_time_ns();
            last_message_time = now;

            if (first_sample) {
                stats.start_time_ns = now;
                last_report_time = now;
                first_sample = false;
                printf("First sample received. Starting measurements...\n");
            }

            PerformanceData data;
            if (deserialize_performance_data(buffer, data_size, &data) == 0) {
                if (out_of_order) {
                    /* Out-of-order mode: count all unique samples */
                    if (stats_add_seen_seq(&stats, data.seq_num)) {
                        stats.total_samples++;
                    }
                } else {
                    /* In-order mode: track expected seq and detect loss */
                    stats_add_seen_seq(&stats, data.seq_num);
                    if (data.seq_num >= stats.expected_seq) {
                        stats.lost_samples += data.seq_num - stats.expected_seq;
                        stats.expected_seq = data.seq_num + 1;
                        stats.total_samples++;
                    }
                }
            }

            /* Periodic report every 5 seconds */
            double elapsed = (double)(now - stats.start_time_ns) / 1000000000.0;
            int current_interval = (int)(elapsed / 5.0);
            if (current_interval > last_report_interval && elapsed >= 5.0) {
                stats.end_time_ns = now;
                if (out_of_order) {
                    stats_calculate_lost_from_hashset(&stats);
                }

                double msgs_per_sec, mbps;
                stats_calculate_throughput(&stats, data_len, &msgs_per_sec, &mbps);
                uint64_t max_seq = stats_get_max_seq(&stats);
                double loss_rate = (max_seq > 0) ? (stats.lost_samples * 100.0 / max_seq) : 0.0;

                printf("[%ds] Received: %llu, Rate: %.0f msg/s, %.2f Mbps, Lost: %llu, Last seq: %llu, Loss rate: %.2f%%\n",
                       current_interval * 5,
                       (unsigned long long)stats.total_samples,
                       msgs_per_sec, mbps,
                       (unsigned long long)stats.lost_samples,
                       (unsigned long long)max_seq,
                       loss_rate);

                last_report_interval = current_interval;
            }
        } else if (ret == INT2DDS_RET_NO_DATA) {
            /* Check for timeout (2 seconds without data) */
            if (!first_sample && last_message_time > 0) {
                uint64_t now = get_current_time_ns();
                if (now - last_message_time >= 2000000000ULL) {
                    printf("\n2 second timeout after last message. Stopping...\n");
                    break;
                }
            }
            sleep_us(100);
        }
    }

    /* Final interval report */
    {
        uint64_t now = get_current_time_ns();
        stats.end_time_ns = now;
        if (out_of_order) {
            stats_calculate_lost_from_hashset(&stats);
        }

        double elapsed = (double)(now - stats.start_time_ns) / 1000000000.0;
        double msgs_per_sec_final, mbps_final;
        stats_calculate_throughput(&stats, data_len, &msgs_per_sec_final, &mbps_final);
        uint64_t max_seq = stats_get_max_seq(&stats);
        double loss_rate_final = (max_seq > 0) ? (stats.lost_samples * 100.0 / max_seq) : 0.0;

        printf("[%.1fs] Received: %llu, Rate: %.0f msg/s, %.2f Mbps, Lost: %llu, Last seq: %llu, Loss rate: %.2f%%\n",
               elapsed,
               (unsigned long long)stats.total_samples,
               msgs_per_sec_final, mbps_final,
               (unsigned long long)stats.lost_samples,
               (unsigned long long)max_seq,
               loss_rate_final);
    }

    /* Final statistics */
    stats.end_time_ns = last_message_time;
    if (out_of_order) {
        stats_calculate_lost_from_hashset(&stats);
    }
    stats_print_summary(&stats, data_len);

    /* Save to CSV */
    double msgs_per_sec, mbps;
    stats_calculate_throughput(&stats, data_len, &msgs_per_sec, &mbps);
    uint64_t max_seq = stats_get_max_seq(&stats);
    double loss_rate = (max_seq > 0) ? (stats.lost_samples * 100.0 / max_seq) : 0.0;

    save_throughput_csv("subscriber", data_len, hz,
                        stats.total_samples, msgs_per_sec, mbps,
                        stats.lost_samples, loss_rate);

    free(buffer);

cleanup:
    stats_free(&stats);
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ==================== Latency Test (Echo Mode) ==================== */

static void run_latency_subscriber(int32_t domain_id, const char* reliability, int execution_time) {
    printf("Starting latency subscriber (echo mode):\n");
    printf("  Reliability: %s\n", reliability);
    printf("  Execution time: %d seconds (from first sample)\n", execution_time);

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

    uint64_t total_echoes = 0;
    uint64_t start_time_ns = 0;

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "latency_subscriber", domain_id, &participant);
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

    /* Create DataReader and DataWriter */
    ret = int2dds_create_datareader(subscriber, topic, reader_qos, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datawriter(publisher, echo_topic, writer_qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for latency test publisher...\n");
    printf("Ready to echo latency measurements back to publisher\n");

    /* Allocate buffer */
    size_t buffer_size = 24 + 65536; /* Max payload size */
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (!buffer) {
        fprintf(stderr, "Failed to allocate buffer\n");
        goto cleanup;
    }

    /* Wait for first sample */
    bool first_sample = true;
    uint64_t last_report_time = 0;

    /* Main echo loop */
    while (1) {
        size_t data_size = 0;
        bool valid_data = false;
        ret = int2dds_take(reader, buffer, buffer_size, &data_size, &valid_data);

        if (ret == INT2DDS_RET_OK && valid_data) {
            if (first_sample) {
                start_time_ns = get_current_time_ns();
                last_report_time = start_time_ns;
                first_sample = false;
                printf("First latency sample received. Starting %d second timer.\n", execution_time);
            }

            /* Echo data back (send exact same data) */
            ret = int2dds_write(writer, buffer, data_size);
            if (ret == INT2DDS_RET_OK) {
                total_echoes++;
            }

            /* Periodic report every 5 seconds */
            uint64_t now = get_current_time_ns();
            if (now - last_report_time >= 5000000000ULL) {
                double elapsed = (double)(now - start_time_ns) / 1000000000.0;
                double rate = total_echoes / elapsed;
                printf("[%.1fs] Echoes sent: %llu, Rate: %.0f echo/s\n",
                       elapsed, (unsigned long long)total_echoes, rate);
                last_report_time = now;
            }

            /* Check timeout */
            if (!first_sample) {
                uint64_t now = get_current_time_ns();
                if (now - start_time_ns >= (uint64_t)execution_time * 1000000000ULL) {
                    break;
                }
            }
        } else if (ret == INT2DDS_RET_NO_DATA) {
            /* Check execution time */
            if (!first_sample) {
                uint64_t now = get_current_time_ns();
                if (now - start_time_ns >= (uint64_t)execution_time * 1000000000ULL) {
                    break;
                }
            }
            sleep_us(100);
        }
    }

    /* Final interval report */
    {
        uint64_t now = get_current_time_ns();
        double elapsed = (double)(now - start_time_ns) / 1000000000.0;
        double rate = (elapsed > 0) ? (total_echoes / elapsed) : 0.0;
        printf("[%.1fs] Echoes sent: %llu, Rate: %.0f echo/s\n",
               elapsed, (unsigned long long)total_echoes, rate);
    }

    /* Final statistics */
    uint64_t end_time_ns = get_current_time_ns();
    double duration_sec = (double)(end_time_ns - start_time_ns) / 1000000000.0;
    double echo_rate = (duration_sec > 0) ? (total_echoes / duration_sec) : 0.0;

    printf("\n=== Final Echo Statistics ===\n");
    printf("Total echoes sent: %llu\n", (unsigned long long)total_echoes);
    printf("Test duration: %.2f seconds\n", duration_sec);
    printf("Average echo rate: %.0f echo/s\n", echo_rate);
    printf("(Latency measurements are calculated by publisher)\n");
    printf("\nLatency test completed successfully.\n");

    free(buffer);

cleanup:
    if (participant) {
        int2dds_participant_delete_contained_entities(participant);
        int2dds_delete_participant(participant);
    }
    if (factory) int2dds_domain_participant_factory_finalize(factory);
}

/* ==================== Local Latency Test ==================== */

static void run_local_latency_subscriber(int32_t domain_id, const char* reliability,
                                         int execution_time, size_t data_len, double hz) {
    printf("Starting local latency subscriber:\n");
    printf("  Reliability: %s\n", reliability);
    printf("  Execution time: %d seconds\n", execution_time);

    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;

    PerformanceStats stats;
    stats_init(&stats, 100000);

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "local_latency_subscriber", domain_id, &participant);
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
    ret = int2dds_create_topic(participant, "local_latency_test_topic", "PerformanceData", NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* Create DataReader QoS */
    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    if (strcmp(reliability, "reliable") == 0) {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE);
    } else {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT);
    }
    int2dds_datareader_qos_set_history(qos, INT2DDS_QOS_HISTORY_KEEP_LAST, 1000);

    /* Create DataReader */
    ret = int2dds_create_datareader(subscriber, topic, qos, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Waiting for publisher...\n");

    /* Allocate buffer */
    size_t buffer_size = 16 + data_len + 1024;
    uint8_t* buffer = (uint8_t*)malloc(buffer_size);
    if (!buffer) {
        fprintf(stderr, "Failed to allocate buffer\n");
        goto cleanup;
    }

    /* Test variables */
    bool first_sample = true;
    uint64_t last_message_time = 0;
    int last_report_interval = 0;

    /* Main receive loop */
    while (1) {
        uint64_t receive_time_ns = get_current_time_ns();
        size_t data_size = 0;
        bool valid_data = false;
        ret = int2dds_take(reader, buffer, buffer_size, &data_size, &valid_data);

        if (ret == INT2DDS_RET_OK && valid_data) {
            last_message_time = receive_time_ns;

            if (first_sample) {
                stats.start_time_ns = receive_time_ns;
                first_sample = false;
                printf("First local latency sample received. Starting measurements...\n");
            }

            PerformanceData data;
            if (deserialize_performance_data(buffer, data_size, &data) == 0) {
                /* Calculate one-way latency */
                uint64_t latency_ns = (receive_time_ns > data.timestamp) ?
                                      (receive_time_ns - data.timestamp) : 0;
                stats_add_latency(&stats, latency_ns);
                stats.total_samples++;
                stats_add_seen_seq(&stats, data.seq_num);
            }

            /* Periodic report every 5 seconds */
            double elapsed = (double)(receive_time_ns - stats.start_time_ns) / 1000000000.0;
            int current_interval = (int)(elapsed / 5.0);
            if (current_interval > last_report_interval && elapsed >= 5.0) {
                double avg_latency, min_latency, max_latency;
                stats_calculate_current_latency(&stats, &avg_latency, &min_latency, &max_latency);

                printf("[%ds] Samples: %llu, Avg: %.3f ms, Min: %.3f ms, Max: %.3f ms\n",
                       current_interval * 5,
                       (unsigned long long)stats.latency_count,
                       avg_latency / 1000000.0,
                       min_latency / 1000000.0,
                       max_latency / 1000000.0);

                last_report_interval = current_interval;
            }
        } else if (ret == INT2DDS_RET_NO_DATA) {
            /* Check for timeout (2 seconds without data) */
            if (!first_sample && last_message_time > 0) {
                uint64_t now = get_current_time_ns();
                if (now - last_message_time >= 2000000000ULL) {
                    printf("\n2 second timeout after last message. Stopping...\n");
                    break;
                }
            }
            sleep_us(100);
        }
    }

    /* Final interval report */
    {
        uint64_t now = get_current_time_ns();
        double elapsed = (double)(now - stats.start_time_ns) / 1000000000.0;
        double avg_latency, min_latency, max_latency;
        stats_calculate_current_latency(&stats, &avg_latency, &min_latency, &max_latency);

        printf("[%.1fs] Samples: %llu, Avg: %.3f ms, Min: %.3f ms, Max: %.3f ms\n",
               elapsed,
               (unsigned long long)stats.latency_count,
               avg_latency / 1000000.0,
               min_latency / 1000000.0,
               max_latency / 1000000.0);
    }

    /* Final statistics */
    stats.end_time_ns = last_message_time;
    double duration_secs = (double)(stats.end_time_ns - stats.start_time_ns) / 1000000000.0;

    printf("\n=== Local Latency Test Results ===\n");
    printf("Test duration: %.2f seconds\n", duration_secs);
    printf("Total samples: %llu\n", (unsigned long long)stats.total_samples);

    if (stats.latency_count > 0) {
        double avg_latency, min_latency, max_latency;
        stats_calculate_latency(&stats, &avg_latency, &min_latency, &max_latency);

        printf("Latency samples: %zu\n", stats.latency_count);
        printf("Average latency: %.3f ms\n", avg_latency / 1000000.0);
        printf("Min latency: %.3f ms\n", min_latency / 1000000.0);
        printf("Max latency: %.3f ms\n", max_latency / 1000000.0);

        /* Save to CSV */
        save_local_latency_csv("subscriber", data_len, hz,
                               stats.latency_count,
                               avg_latency / 1000000.0,
                               min_latency / 1000000.0,
                               max_latency / 1000000.0);
    }

    free(buffer);

cleanup:
    stats_free(&stats);
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
    printf("  --out-of-order, -o       Enable out-of-order mode for throughput test\n");
    printf("  --hz, -z <rate>          Rate limit in Hz (for CSV filename)\n");
    printf("  --help, -h               Show this help\n");
}

int main(int argc, char* argv[]) {
    /* Default parameters */
    char test_mode[64] = "throughput";
    int32_t domain_id = 17;
    size_t data_len = 1024;
    int execution_time = 30;
    char reliability[64] = "best_effort";
    bool out_of_order = false;
    double hz = 0.0;

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
        } else if (strcmp(argv[i], "--out-of-order") == 0 || strcmp(argv[i], "-o") == 0) {
            out_of_order = true;
        } else if (strcmp(argv[i], "--hz") == 0 || strcmp(argv[i], "-z") == 0) {
            if (i + 1 < argc) {
                hz = atof(argv[++i]);
            }
        } else if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "-h") == 0) {
            print_help(argv[0]);
            return 0;
        }
    }

    printf("int2dds FFI Performance Test Subscriber\n");
    printf("Test mode: %s\n", test_mode);

    if (strcmp(test_mode, "throughput") == 0 || strcmp(test_mode, "thr") == 0 || strcmp(test_mode, "1") == 0) {
        printf("Starting throughput test mode\n");
        run_throughput_subscriber(domain_id, reliability, execution_time, out_of_order, data_len, hz);
    } else if (strcmp(test_mode, "latency") == 0 || strcmp(test_mode, "lat") == 0 || strcmp(test_mode, "2") == 0) {
        printf("Starting latency test mode\n");
        run_latency_subscriber(domain_id, reliability, execution_time);
    } else if (strcmp(test_mode, "local_latency") == 0 || strcmp(test_mode, "local") == 0 ||
               strcmp(test_mode, "ll") == 0 || strcmp(test_mode, "3") == 0) {
        printf("Starting local latency test mode\n");
        run_local_latency_subscriber(domain_id, reliability, execution_time, data_len, hz);
    } else {
        printf("Invalid test mode '%s'. Supported modes: throughput, latency, local_latency\n", test_mode);
        printf("Defaulting to throughput test.\n");
        run_throughput_subscriber(domain_id, reliability, execution_time, out_of_order, data_len, hz);
    }

    return 0;
}
