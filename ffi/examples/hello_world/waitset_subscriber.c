/**
 * WaitSet Example Subscriber
 *
 * Demonstrates using WaitSet to wait for data availability
 * instead of polling.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include "../include/int2dds-ffi.h"

// Simple deserialization for HelloWorld (index: u32, message: String)
int deserialize_hello_world(const uint8_t* data, size_t data_size,
                            uint32_t* index, char* message, size_t message_buf_size) {
    if (data_size < 8) return -1;  // minimum: 4 (index) + 4 (string length)

    // Read index (little-endian u32)
    *index = data[0] | (data[1] << 8) | (data[2] << 16) | (data[3] << 24);

    // Read string length (little-endian u32)
    uint32_t str_len = data[4] | (data[5] << 8) | (data[6] << 16) | (data[7] << 24);

    if (data_size < 8 + str_len) return -1;

    // Copy string (truncate if necessary)
    size_t copy_len = (str_len < message_buf_size - 1) ? str_len : message_buf_size - 1;
    memcpy(message, data + 8, copy_len);
    message[copy_len] = '\0';

    return 0;
}

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsWaitSet* waitset = NULL;

    int domain_id = 0;

    printf("=== int2dds WaitSet Subscriber Example ===\n\n");

    // Initialize
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to get participant factory: %d\n", ret);
        return 1;
    }

    // Create Participant (DomainParticipant)
    ret = int2dds_create_participant(factory, NULL, domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create participant: %d\n", ret);
        int2dds_domain_participant_factory_finalize(factory);
        return 1;
    }
    printf("Created participant on domain %d\n", domain_id);

    // Create Subscriber
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    // Create Topic
    ret = int2dds_create_topic(participant, "HelloWorldTopic", "HelloWorld", NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create topic: %d\n", ret);
        goto cleanup;
    }
    printf("Created topic: HelloWorldTopic\n");

    // Create DataReader with default QoS
    ret = int2dds_create_datareader(subscriber, topic, NULL, &reader);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create data reader: %d\n", ret);
        goto cleanup;
    }
    printf("Created data reader\n");

    // Create WaitSet
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create waitset: %d\n", ret);
        goto cleanup;
    }
    printf("Created waitset\n");

    // Attach DataReader's status condition to WaitSet
    ret = int2dds_waitset_attach_datareader(waitset, reader);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to attach datareader to waitset: %d\n", ret);
        goto cleanup;
    }
    printf("Attached datareader to waitset\n\n");

    // Wait for publisher to match using WaitSet
    printf("Waiting for publisher (using WaitSet)...\n");
    ret = int2dds_waitset_wait(waitset, -1);  // -1 for infinite wait
    if (ret != INT2DDS_RET_OK) {
        printf("WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    // Get matched status to reset condition
    int32_t total_count = 0;
    int32_t current_count = 0;
    ret = int2dds_get_subscription_matched_status(reader, &total_count, &current_count);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to get subscription matched status: %d\n", ret);
        goto cleanup;
    }
    printf("Publisher matched! (total: %d, current: %d)\n", total_count, current_count);
    printf("Waiting for data...\n\n");

    // Read all available samples
    uint8_t data[65536];
    size_t data_size = 0;
    int received_count = 0;

    while (1) {
        bool valid_data = false;
        ret = int2dds_take(reader, data, sizeof(data), &data_size, &valid_data);

        if (ret == INT2DDS_RET_NO_DATA) {
            continue;
        } else if (ret != INT2DDS_RET_OK) {
            printf("Failed to take data: %d\n", ret);
            break;
        }

        if (!valid_data) continue;

        // Deserialize and display
        uint32_t index;
        char message[256];

        if (deserialize_hello_world(data, data_size, &index, message, sizeof(message)) == 0) {
            printf("[%d] Received: index=%u, message=\"%s\"\n",
                   received_count + 1, index, message);
            received_count++;
        } else {
            printf("Failed to deserialize message\n");
        }
    }

    printf("\nReceived %d messages\n", received_count);

cleanup:
    // Cleanup
    if (waitset) {
        int2dds_waitset_detach_datareader(waitset, reader);
        int2dds_waitset_delete(waitset);
    }
    if (reader) int2dds_delete_datareader(reader);
    if (topic) int2dds_delete_topic(topic);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Cleanup complete\n");
    return 0;
}
