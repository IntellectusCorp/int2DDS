/**
 * int2dds FFI Integers Type Publisher
 *
 * Topic: IntegersTestTopic
 * Type:  IntegersType
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
#include "Integers.h"

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

    printf("=== int2dds Integers Publisher ===\n");
    printf("Topic: IntegersTestTopic\n");
    printf("Type:  IntegersType (APPENDABLE)\n");
    printf("Domain: %d, QoS: %s\n", domain_id,
           use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("Encoding: %s\n", use_xcdr2 ? "XCDR2" : "XCDR1");
    printf("Count: %s\n", count > 0 ? "" : "infinite");
    if (count > 0) printf("  %d samples\n", count);
    printf("==================================\n\n");

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    ret = int2dds_create_participant(factory, "integers_publisher", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    {
        Int2DdsTypeInfo *type_info = IntegersType_type_info();
        ret = int2dds_create_topic_with_type_info(participant, "IntegersTestTopic",
                                                   type_info, NULL, &topic);
        int2dds_type_info_destroy(type_info);
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    if (use_reliable) {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 100000000);
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    if (use_xcdr2) {
        ret = int2dds_datawriter_qos_set_data_representation(qos, INT2DDS_QOS_DATA_REPR_XCDR2);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to set data representation: %d\n", ret);
            goto cleanup;
        }
    }

    Int2DdsDataWriterListener listener = {0};
    listener.on_publication_matched = on_publication_matched;

    ret = int2dds_create_datawriter_with_listener(
        publisher, topic, qos, &listener,
        INT2DDS_STATUS_PUBLICATION_MATCHED,
        &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("Publisher ready. Waiting for subscriber...\n");
    printf("Press Ctrl+C to stop.\n\n");

    IntegersType data;
    uint8_t buf[256];
    uint8_t key_buf[16];
    uint32_t sample_index = 0;

    while (g_running) {
        if (count > 0 && (int32_t)sample_index >= count) break;

        data.id = 1;
        data.i8_val = (int8_t)(sample_index % 128);
        data.i16_val = (int16_t)(sample_index * 10);
        data.i32_val = (int32_t)(sample_index * 100);
        data.i64_val = (int64_t)(sample_index * 1000);
        data.u8_val = (uint8_t)(sample_index % 256);
        data.u16_val = (uint16_t)(sample_index * 10);
        data.u32_val = (uint32_t)(sample_index * 100);
        data.u64_val = (uint64_t)(sample_index * 1000);

        size_t serialized_len = IntegersType_serialize_cdr(&data, buf, sizeof(buf), use_xcdr2);
        if (serialized_len == 0) {
            fprintf(stderr, "Serialization failed\n");
            sample_index++;
            continue;
        }

        size_t key_len = IntegersType_serialize_key(&data, key_buf, sizeof(key_buf));

        ret = int2dds_write_serialized(writer, buf, serialized_len, key_buf, key_len);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Write failed: %d\n", ret);
        } else {
            printf("[%u] Sent IntegersType (i8=%d, i16=%d, i32=%d, i64=%lld, u8=%u, u16=%u, u32=%u, u64=%llu)\n",
                   sample_index, data.i8_val, data.i16_val, data.i32_val,
                   (long long)data.i64_val, data.u8_val, data.u16_val,
                   data.u32_val, (unsigned long long)data.u64_val);
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
