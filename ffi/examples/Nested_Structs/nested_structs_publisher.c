/**
 * int2dds FFI Nested_Structs Type Publisher
 * Sub-structs (InnerStruct, MiddleStruct, DeepInnerStruct) are FINAL.
 *
 * Topic: NestedStructTestTopic
 * Type:  NestedStructType
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
#include "Nested_Structs.h"

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

    printf("=== int2dds Nested_Structs Publisher ===\n");
    printf("Topic: NestedStructTestTopic\n");
    printf("Type:  NestedStructType (APPENDABLE)\n");
    printf("Domain: %d, QoS: %s\n", domain_id,
           use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("Encoding: %s\n", use_xcdr2 ? "XCDR2" : "XCDR1");
    printf("Count: %s\n", count > 0 ? "" : "infinite");
    if (count > 0) printf("  %d samples\n", count);
    printf("=========================================\n\n");

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to get participant factory: %d\n", ret); return 1; }

    ret = int2dds_create_participant(factory, "nested_structs_publisher", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create participant: %d\n", ret); goto cleanup; }

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create publisher: %d\n", ret); goto cleanup; }

    {
        Int2DdsTypeInfo *type_info = NestedStructType_type_info();
        ret = int2dds_create_topic_with_type_info(participant, "NestedStructTestTopic",
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

    InnerStruct seq_buf[10];
    NestedStructType data;
    uint8_t buf[4096];
    uint8_t key_buf[16];
    uint32_t sample_index = 0;

    while (g_running) {
        if (count > 0 && (int32_t)sample_index >= count) break;

        data.id = 1;

        /* simple_nested: InnerStruct */
        data.simple_nested.x = (int32_t)(sample_index * 10);
        data.simple_nested.y = (int32_t)(sample_index * 20);
        snprintf(data.simple_nested.name, sizeof(data.simple_nested.name),
                 "nested_%u", sample_index);

        /* deep_nested: MiddleStruct -> DeepInnerStruct */
        data.deep_nested.level = (int32_t)sample_index;
        data.deep_nested.deep.value = (int32_t)(sample_index * 100);
        snprintf(data.deep_nested.deep.description,
                 sizeof(data.deep_nested.deep.description),
                 "deep_level_%u", sample_index);

        /* struct_seq: growing sequence of InnerStruct */
        uint32_t seq_len = (sample_index % 3) + 1;
        for (uint32_t j = 0; j < seq_len; j++) {
            seq_buf[j].x = (int32_t)(sample_index + j);
            seq_buf[j].y = (int32_t)((sample_index + j) * 2);
            snprintf(seq_buf[j].name, sizeof(seq_buf[j].name),
                     "item_%u_%u", sample_index, j);
        }
        data.struct_seq.data = seq_buf;
        data.struct_seq.length = seq_len;

        size_t serialized_len = NestedStructType_serialize_cdr(&data, buf, sizeof(buf), use_xcdr2);
        if (serialized_len == 0) { fprintf(stderr, "Serialization failed\n"); sample_index++; continue; }

        size_t key_len = NestedStructType_serialize_key(&data, key_buf, sizeof(key_buf));

        ret = int2dds_write_serialized(writer, buf, serialized_len, key_buf, key_len);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Write failed: %d\n", ret);
        } else {
            printf("[%u] Sent NestedStructType (simple={x=%d,y=%d,name=\"%s\"}, "
                   "deep={level=%d,deep={value=%d}}, struct_seq.len=%u)\n",
                   sample_index,
                   data.simple_nested.x, data.simple_nested.y, data.simple_nested.name,
                   data.deep_nested.level, data.deep_nested.deep.value,
                   data.struct_seq.length);
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
