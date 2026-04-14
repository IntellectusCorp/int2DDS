/**
 * Multi-Participant Example 1
 *
 * Domain 0: Publisher
 * Domain 1: Subscriber
 *
 * Demonstrates cross-domain communication (NOTE: domains don't communicate by default).
 * This example shows how to create participants in different domains.
 * Run with multi_participant_2 to test within same domain.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef _WIN32
#include <windows.h>
#define sleep_ms(ms) Sleep(ms)
#else
#include <unistd.h>
#define sleep_ms(ms) usleep((ms) * 1000)
#endif

#define INT2DDS_CDR_STATIC
#include "int2dds-ffi.h"
#include "int2dds_cdr.h"

/* MultiDomainData type */
typedef struct MultiDomainData {
    uint32_t index;
    char message[257];
} MultiDomainData;

static size_t MultiDomainData_serialize_cdr(const MultiDomainData *val, uint8_t *buf, size_t capacity) {
    Int2DdsCdrWriter w;
    int2dds_cdr_writer_init(&w, buf, capacity, true, true);
    int2dds_cdr_write_encapsulation(&w, INT2DDS_CDR_APPENDABLE);
    size_t dh = 0;
    int2dds_cdr_write_dheader_begin(&w, &dh);
    int2dds_cdr_write_u32(&w, val->index);
    int2dds_cdr_write_string(&w, val->message);
    int2dds_cdr_write_dheader_finalize(&w, dh);
    return w.error == INT2DDS_CDR_OK ? int2dds_cdr_writer_size(&w) : 0;
}

static bool MultiDomainData_deserialize_cdr(const uint8_t *buf, size_t len, MultiDomainData *val_out) {
    Int2DdsCdrReader r;
    if (int2dds_cdr_reader_init(&r, buf, len) != INT2DDS_CDR_OK)
        return false;
    uint32_t obj_size = 0;
    size_t start_pos = 0;
    int2dds_cdr_read_dheader(&r, &obj_size, &start_pos);
    int2dds_cdr_read_u32(&r, &val_out->index);
    int2dds_cdr_read_string_copy(&r, val_out->message, 257, NULL);
    int2dds_cdr_read_dheader_end(&r, obj_size, start_pos);
    return int2dds_cdr_reader_error(&r) == INT2DDS_CDR_OK;
}


int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;

    // Publisher on domain 0
    Int2DdsParticipant* pub_participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* pub_topic = NULL;
    Int2DdsDataWriter* writer = NULL;

    // Subscriber on domain 1
    Int2DdsParticipant* sub_participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* sub_topic = NULL;
    Int2DdsDataReader* reader = NULL;

    printf("=== Multi-Participant Example 1 ===\n");
    printf("Publisher: Domain 0\n");
    printf("Subscriber: Domain 1\n\n");

    // Initialize
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to get participant factory: %d\n", ret);
        return 1;
    }

    // Create publisher participant on domain 0
    ret = int2dds_create_participant(factory, "multi_part1_publisher", 0, &pub_participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher participant: %d\n", ret);
        goto cleanup;
    }
    printf("Created publisher participant on domain 0\n");

    // Create subscriber participant on domain 1
    ret = int2dds_create_participant(factory, "multi_part1_subscriber", 1, &sub_participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber participant: %d\n", ret);
        goto cleanup;
    }
    printf("Created subscriber participant on domain 1\n");

    // Create publisher
    ret = int2dds_create_publisher(pub_participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    // Create subscriber
    ret = int2dds_create_subscriber(sub_participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    // Create topics (Appendable extensibility)
    ret = int2dds_create_topic(pub_participant, "MultiDomainTopic", "MultiDomainData",
                               1,  /* APPENDABLE */
                               NULL, &pub_topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(sub_participant, "MultiDomainTopic", "MultiDomainData",
                               1,  /* APPENDABLE */
                               NULL, &sub_topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber topic: %d\n", ret);
        goto cleanup;
    }

    // Create writer and reader
    ret = int2dds_create_datawriter(publisher, pub_topic, NULL, &writer);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create writer: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datareader(subscriber, sub_topic, NULL, &reader);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create reader: %d\n", ret);
        goto cleanup;
    }

    printf("\nNOTE: Different domains don't communicate by default.\n");
    printf("This example: Publisher domain 0, Subscriber domain 1\n");
    printf("multi_participant_2: Publisher domain 1, Subscriber domain 0 (opposite)\n\n");

    // Send messages (won't be received due to different domains)
    {
        MultiDomainData data;
        uint8_t buf[4096];
        uint8_t recv_buf[4096];

        printf("Sending messages on domain 0...\n");
        for (uint32_t i = 0; i < 5; i++) {
            data.index = i;
            snprintf(data.message, sizeof(data.message), "Message from domain 0 [%u]", i);

            size_t serialized_len = MultiDomainData_serialize_cdr(&data, buf, sizeof(buf));
            if (serialized_len == 0) {
                printf("Serialization failed\n");
                continue;
            }

            ret = int2dds_write_serialized(writer, buf, serialized_len, NULL, 0);
            if (ret == INT2DDS_RET_OK) {
                printf("[%u] Sent: %s\n", i, data.message);
            }
            sleep_ms(500);
        }

        // Try to receive messages
        printf("\nTrying to receive on domain 1...\n");
        uintptr_t actual_size;
        bool valid_data;
        int received = 0;

        while (1) {
            ret = int2dds_take_serialized(reader, recv_buf, sizeof(recv_buf),
                                          &actual_size, &valid_data);
            if (ret == INT2DDS_RET_NO_DATA) {
                break;
            } else if (ret == INT2DDS_RET_OK && valid_data) {
                MultiDomainData recv_data;
                if (MultiDomainData_deserialize_cdr(recv_buf, actual_size, &recv_data)) {
                    printf("[%u] Received: %s\n", recv_data.index, recv_data.message);
                    received++;
                }
            }
        }

        if (received == 0) {
            printf("No messages received (expected - different domains)\n");
        }
    }

    printf("\nExample complete.\n");

cleanup:
    if (reader) int2dds_delete_datareader(reader);
    if (writer) int2dds_delete_datawriter(writer);
    if (sub_topic) int2dds_delete_topic(sub_topic);
    if (pub_topic) int2dds_delete_topic(pub_topic);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (publisher) int2dds_delete_publisher(publisher);
    if (sub_participant) int2dds_delete_participant(sub_participant);
    if (pub_participant) int2dds_delete_participant(pub_participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    return 0;
}
