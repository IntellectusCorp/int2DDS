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

#include "../include/int2dds-ffi.h"


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

    // Type descriptors and data
    Int2DdsTypeDescriptor* pub_type_desc = NULL;
    Int2DdsTypeDescriptor* sub_type_desc = NULL;
    Int2DdsData* pub_data = NULL;
    Int2DdsData* sub_data = NULL;

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
    ret = int2dds_create_participant(factory, NULL, 0, &pub_participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher participant: %d\n", ret);
        goto cleanup;
    }
    printf("Created publisher participant on domain 0\n");

    // Create subscriber participant on domain 1
    ret = int2dds_create_participant(factory, NULL, 1, &sub_participant);
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

    // Create type descriptor for publisher
    ret = int2dds_type_descriptor_create("MultiDomainData", &pub_type_desc);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher type descriptor: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_type_descriptor_add_u32(pub_type_desc, "index", false);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to add index field: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_type_descriptor_add_string(pub_type_desc, "message", 256, false);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to add message field: %d\n", ret);
        goto cleanup;
    }

    // Create type descriptor for subscriber
    ret = int2dds_type_descriptor_create("MultiDomainData", &sub_type_desc);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber type descriptor: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_type_descriptor_add_u32(sub_type_desc, "index", false);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to add index field: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_type_descriptor_add_string(sub_type_desc, "message", 256, false);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to add message field: %d\n", ret);
        goto cleanup;
    }

    // Create topics with type descriptors
    ret = int2dds_create_topic(pub_participant, "MultiDomainTopic", pub_type_desc, NULL, &pub_topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(sub_participant, "MultiDomainTopic", sub_type_desc, NULL, &sub_topic);
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

    // Send messages (won't be received due to different domains)
    printf("Sending messages on domain 0...\n");
    char message[64];
    for (uint32_t i = 0; i < 5; i++) {
        snprintf(message, sizeof(message), "Message from domain 0 [%u]", i);

        ret = int2dds_data_set_u32(pub_data, "index", i);
        if (ret != INT2DDS_RET_OK) {
            printf("Failed to set index: %d\n", ret);
            continue;
        }

        ret = int2dds_data_set_string(pub_data, "message", message);
        if (ret != INT2DDS_RET_OK) {
            printf("Failed to set message: %d\n", ret);
            continue;
        }

        ret = int2dds_write(writer, pub_data);
        if (ret == INT2DDS_RET_OK) {
            printf("[%u] Sent: %s\n", i, message);
        }
        sleep_ms(500);
    }

    // Try to receive messages
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

            printf("[%u] Received: %s\n", index, recv_message);
        }
    }

    printf("\nExample complete.\n");

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
