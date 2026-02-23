/**
 * int2dds FFI Arrays Type Publisher
 *
 * Topic: ArraysTestTopic
 * Type:  ArraysType
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <signal.h>

#ifdef _WIN32
#include <windows.h>
#define sleep_ms(ms) Sleep(ms)
#else
#include <unistd.h>
#define sleep_ms(ms) usleep((ms) * 1000)
#endif

#define INT2DDS_CDR_STATIC
#include "int2dds-ffi.h"
#include "Arrays.h"

static volatile sig_atomic_t g_running = 1;

static void signal_handler(int sig) {
    (void)sig;
    g_running = 0;
}

static void on_publication_matched(
    Int2DdsDataWriter *writer,
    const Int2DdsPublicationMatchedStatus *status,
    void *user_ctx)
{
    (void)writer;
    (void)user_ctx;
    if (status->current_count_change > 0) {
        printf("[Matched] Subscriber connected (current: %d, total: %d)\n",
               status->current_count, status->total_count);
    } else if (status->current_count_change < 0) {
        printf("[Matched] Subscriber disconnected (current: %d, total: %d)\n",
               status->current_count, status->total_count);
    }
}

static void print_usage(const char *prog) {
    printf("Usage: %s [options]\n", prog);
    printf("Options:\n");
    printf("  --domain-id <id>   Domain ID (default: 0)\n");
    printf("  --count <n>        Number of samples to send, 0=infinite (default: 0)\n");
    printf("  --reliability <r>  best_effort or reliable (default: best_effort)\n");
    printf("  --encoding <e>     xcdr1 or xcdr2 (default: xcdr1)\n");
    printf("  --help             Show this help\n");
}

int main(int argc, char *argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsPublisher *publisher = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsDataWriter *writer = NULL;
    Int2DdsDataWriterQos *qos = NULL;

    int32_t domain_id = 0;
    int32_t count = 0;
    int use_reliable = 0;
    int use_xcdr2 = 0;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--domain-id") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--count") == 0 && i + 1 < argc) {
            count = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--reliability") == 0 && i + 1 < argc) {
            i++;
            if (strcmp(argv[i], "reliable") == 0) use_reliable = 1;
        } else if (strcmp(argv[i], "--encoding") == 0 && i + 1 < argc) {
            i++;
            if (strcmp(argv[i], "xcdr2") == 0) use_xcdr2 = 1;
        } else if (strcmp(argv[i], "--help") == 0) {
            print_usage(argv[0]);
            return 0;
        }
    }

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("=== int2dds Arrays Publisher ===\n");
    printf("Topic: ArraysTestTopic\n");
    printf("Type:  ArraysType (APPENDABLE)\n");
    printf("Domain: %d, QoS: %s\n", domain_id,
           use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("Encoding: %s\n", use_xcdr2 ? "XCDR2" : "XCDR1");
    printf("Count: %s\n", count > 0 ? "" : "infinite");
    if (count > 0) printf("  %d samples\n", count);
    printf("================================\n\n");

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to get participant factory: %d\n", ret); return 1; }

    ret = int2dds_create_participant(factory, "arrays_publisher", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create participant: %d\n", ret); goto cleanup; }

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create publisher: %d\n", ret); goto cleanup; }

    {
        Int2DdsTypeInfo *type_info = ArraysType_type_info();
        ret = int2dds_create_topic_with_type_info(participant, "ArraysTestTopic",
                                                   type_info, NULL, &topic);
        int2dds_type_info_destroy(type_info);
    }
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create topic: %d\n", ret); goto cleanup; }

    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create QoS: %d\n", ret); goto cleanup; }

    if (use_reliable) {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 100000000);
    }
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to set reliability: %d\n", ret); goto cleanup; }

    if (use_xcdr2) {
        ret = int2dds_datawriter_qos_set_data_representation(qos, INT2DDS_QOS_DATA_REPR_XCDR2);
        if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to set data representation: %d\n", ret); goto cleanup; }
    }

    Int2DdsDataWriterListener listener = {0};
    listener.on_publication_matched = on_publication_matched;

    ret = int2dds_create_datawriter_with_listener(
        publisher, topic, qos, &listener,
        INT2DDS_STATUS_PUBLICATION_MATCHED, &writer);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datawriter: %d\n", ret); goto cleanup; }

    printf("Publisher ready. Waiting for subscriber...\n");
    printf("Press Ctrl+C to stop.\n\n");

    ArraysType data;
    uint8_t buf[512];
    uint8_t key_buf[16];
    uint32_t sample_index = 0;

    while (g_running) {
        if (count > 0 && (int32_t)sample_index >= count) break;

        data.id = 1;
        data.bool_array[0] = true;
        data.bool_array[1] = false;
        data.bool_array[2] = true;
        data.bool_array[3] = false;
        for (int j = 0; j < 8; j++) data.byte_array[j] = (uint8_t)((sample_index + j) % 256);
        for (int j = 0; j < 4; j++) data.i32_array[j] = (int32_t)(sample_index + j);
        data.f32_array[0] = 0.0f;
        data.f32_array[1] = 1.0f;
        data.f32_array[2] = -1.0f;
        data.f32_array[3] = 3.14f;
        data.f64_array[0] = 0.0;
        data.f64_array[1] = 1.0;
        data.f64_array[2] = -1.0;
        data.f64_array[3] = 2.71;

        size_t serialized_len = ArraysType_serialize_cdr(&data, buf, sizeof(buf), use_xcdr2);
        if (serialized_len == 0) { fprintf(stderr, "Serialization failed\n"); sample_index++; continue; }

        size_t key_len = ArraysType_serialize_key(&data, key_buf, sizeof(key_buf));

        ret = int2dds_write_serialized(writer, buf, serialized_len, key_buf, key_len);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Write failed: %d\n", ret);
        } else {
            printf("[%u] Sent ArraysType (i32=[%d,%d,%d,%d], f32=[%.2f,%.2f,%.2f,%.2f])\n",
                   sample_index,
                   data.i32_array[0], data.i32_array[1], data.i32_array[2], data.i32_array[3],
                   data.f32_array[0], data.f32_array[1], data.f32_array[2], data.f32_array[3]);
        }

        sample_index++;
        sleep_ms(100);
    }

cleanup:
    printf("\nShutting down...\n");
    if (writer) int2dds_delete_datawriter(writer);
    if (qos) int2dds_datawriter_qos_destroy(qos);
    if (topic) int2dds_delete_topic(topic);
    if (publisher) int2dds_delete_publisher(publisher);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Publisher finished. Sent %u samples.\n", sample_index);
    return 0;
}
