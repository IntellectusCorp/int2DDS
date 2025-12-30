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

#include "../include/int2dds-ffi.h"


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

    // Type descriptors and data
    Int2DdsTypeDescriptor* pub_type_desc = NULL;
    Int2DdsTypeDescriptor* sub_type_desc = NULL;
    Int2DdsData* pub_data = NULL;
    Int2DdsData* sub_data = NULL;

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
    ret = int2dds_create_participant(factory, NULL, pub_domain, &pub_participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher participant: %d\n", ret);
        goto cleanup;
    }
    printf("Created publisher participant on domain %d\n", pub_domain);

    // Create subscriber participant on domain 0
    ret = int2dds_create_participant(factory, NULL, sub_domain, &sub_participant);
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

    // Create type descriptor for publisher
    ret = int2dds_type_descriptor_create("SameDomainData", &pub_type_desc);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher type descriptor: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_type_descriptor_add_u32(pub_type_desc, "index", false);
    if (ret != INT2DDS_RET_OK) goto cleanup;

    ret = int2dds_type_descriptor_add_string(pub_type_desc, "message", 256, false);
    if (ret != INT2DDS_RET_OK) goto cleanup;

    // Create type descriptor for subscriber
    ret = int2dds_type_descriptor_create("SameDomainData", &sub_type_desc);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber type descriptor: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_type_descriptor_add_u32(sub_type_desc, "index", false);
    if (ret != INT2DDS_RET_OK) goto cleanup;

    ret = int2dds_type_descriptor_add_string(sub_type_desc, "message", 256, false);
    if (ret != INT2DDS_RET_OK) goto cleanup;

    // Create topics with type descriptors
    ret = int2dds_create_topic(pub_participant, "SameDomainTopic", pub_type_desc, NULL, &pub_topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(sub_participant, "SameDomainTopic", sub_type_desc, NULL, &sub_topic);
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

    // Create data containers
    ret = int2dds_data_create(pub_type_desc, &pub_data);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher data: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_data_create(sub_type_desc, &sub_data);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber data: %d\n", ret);
        goto cleanup;
    }

    // Send and receive messages
    int received_count = 0;
    uint32_t total_messages = 10;
    char message[64];

    for (uint32_t i = 0; i < total_messages; i++) {
        // Send message
        snprintf(message, sizeof(message), "Multi-participant message %u", i);

        ret = int2dds_data_set_u32(pub_data, "index", i);
        if (ret != INT2DDS_RET_OK) continue;

        ret = int2dds_data_set_string(pub_data, "message", message);
        if (ret != INT2DDS_RET_OK) continue;

        ret = int2dds_write(writer, pub_data);
        if (ret == INT2DDS_RET_OK) {
            printf("Sent: [%u] %s\n", i, message);
        }

        // Try to receive
        while (1) {
            bool valid_data = false;
            ret = int2dds_take(reader, sub_data, &valid_data);

            if (ret == INT2DDS_RET_NO_DATA) {
                break;
            } else if (ret == INT2DDS_RET_OK && valid_data) {
                uint32_t index;
                char recv_message[256];
                size_t message_len;

                ret = int2dds_data_get_u32(sub_data, "index", &index);
                if (ret != INT2DDS_RET_OK) continue;

                ret = int2dds_data_get_string(sub_data, "message", recv_message, sizeof(recv_message), &message_len);
                if (ret != INT2DDS_RET_OK) continue;

                printf("Received: [%u] %s\n", index, recv_message);
                received_count++;
            }
        }
        sleep_ms(500);
    }

    printf("\nSent %u messages, received %d messages\n", total_messages, received_count);

cleanup:
    if (sub_data) int2dds_data_delete(sub_data);
    if (pub_data) int2dds_data_delete(pub_data);
    if (reader) int2dds_delete_datareader(reader);
    if (writer) int2dds_delete_datawriter(writer);
    if (sub_topic) int2dds_delete_topic(sub_topic);
    if (pub_topic) int2dds_delete_topic(pub_topic);
    if (sub_type_desc) int2dds_type_descriptor_delete(sub_type_desc);
    if (pub_type_desc) int2dds_type_descriptor_delete(pub_type_desc);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (publisher) int2dds_delete_publisher(publisher);
    if (sub_participant) int2dds_delete_participant(sub_participant);
    if (pub_participant) int2dds_delete_participant(pub_participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    return 0;
}
