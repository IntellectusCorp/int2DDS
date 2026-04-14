/**
 * Multi-Participant Example 2
 *
 * Publisher on Domain 1, Subscriber on Domain 0 (different domains = no communication)
 * Uses WaitSet for synchronization.
 *
 * Demonstrates:
 * - Multiple participants in different domains (opposite of multi_participant_1)
 * - WaitSet for data notification
 * - Different domains don't communicate
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

/* SameDomainData type */
typedef struct SameDomainData {
    uint32_t index;
    char message[257];
} SameDomainData;

static size_t SameDomainData_serialize_cdr(const SameDomainData *val, uint8_t *buf, size_t capacity) {
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

static bool SameDomainData_deserialize_cdr(const uint8_t *buf, size_t len, SameDomainData *val_out) {
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

    // Publisher participant
    Int2DdsParticipant* pub_participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* pub_topic = NULL;
    Int2DdsDataWriter* writer = NULL;

    // Subscriber participant
    Int2DdsParticipant* sub_participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* sub_topic = NULL;
    Int2DdsDataReader* reader = NULL;

    int pub_domain = 1;
    int sub_domain = 0;

    printf("=== Multi-Participant Example 2 ===\n");
    printf("Publisher on Domain %d, Subscriber on Domain %d\n", pub_domain, sub_domain);
    printf("Different domains = no communication (opposite of example 1)\n\n");

    // Initialize
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to get participant factory: %d\n", ret);
        return 1;
    }

    // Create publisher participant on domain 1
    ret = int2dds_create_participant(factory, "multi_part2_publisher", pub_domain, &pub_participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher participant: %d\n", ret);
        goto cleanup;
    }
    printf("Created publisher participant on domain %d\n", pub_domain);

    // Create subscriber participant on domain 0
    ret = int2dds_create_participant(factory, "multi_part2_subscriber", sub_domain, &sub_participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber participant: %d\n", ret);
        goto cleanup;
    }
    printf("Created subscriber participant on domain %d\n", sub_domain);

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
    ret = int2dds_create_topic(pub_participant, "SameDomainTopic", "SameDomainData",
                               1,  /* APPENDABLE */
                               NULL, &pub_topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(sub_participant, "SameDomainTopic", "SameDomainData",
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

    // Wait for matching (will timeout due to different domains)
    printf("\nWaiting for matching (will timeout - different domains)...\n");
    {
        int32_t total_count = 0;
        int32_t current_count = 0;
        int wait_attempts = 0;
        int max_attempts = 30;  // 3 seconds timeout

        while (current_count == 0 && wait_attempts < max_attempts) {
            ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
            if (ret != INT2DDS_RET_OK) {
                printf("Failed to get publication matched status: %d\n", ret);
                goto cleanup;
            }
            if (current_count == 0) {
                sleep_ms(100);
                wait_attempts++;
            }
        }

        if (current_count == 0) {
            printf("No matching (expected - different domains)\n\n");
        } else {
            printf("Matched! (unexpected)\n\n");
        }
    }

    // Send and receive messages
    {
        SameDomainData data;
        uint8_t send_buf[4096];
        uint8_t recv_buf[4096];
        int received_count = 0;
        uint32_t total_messages = 10;

        for (uint32_t i = 0; i < total_messages; i++) {
            // Send message
            data.index = i;
            snprintf(data.message, sizeof(data.message), "Multi-participant message %u", i);

            size_t serialized_len = SameDomainData_serialize_cdr(&data, send_buf, sizeof(send_buf));
            if (serialized_len == 0) {
                printf("Serialization failed\n");
                continue;
            }

            ret = int2dds_write_serialized(writer, send_buf, serialized_len, NULL, 0);
            if (ret == INT2DDS_RET_OK) {
                printf("Sent: [%u] %s\n", i, data.message);
            }

            // Try to receive
            uintptr_t actual_size;
            bool valid_data;
            while (1) {
                ret = int2dds_take_serialized(reader, recv_buf, sizeof(recv_buf),
                                              &actual_size, &valid_data);

                if (ret == INT2DDS_RET_NO_DATA) {
                    break;
                } else if (ret == INT2DDS_RET_OK && valid_data) {
                    SameDomainData recv_data;
                    if (SameDomainData_deserialize_cdr(recv_buf, actual_size, &recv_data)) {
                        printf("Received: [%u] %s\n", recv_data.index, recv_data.message);
                        received_count++;
                    }
                }
            }
            sleep_ms(500);
        }

        printf("\nSent %u messages, received %d messages\n", total_messages, received_count);
    }

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
