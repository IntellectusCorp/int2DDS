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


int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsWaitSet* waitset = NULL;
    Int2DdsTypeDescriptor* type_desc = NULL;
    Int2DdsData* data = NULL;

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

    // Create type descriptor for HelloWorld
    ret = int2dds_type_descriptor_create("HelloWorld", &type_desc);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create type descriptor: %d\n", ret);
        goto cleanup;
    }

    // Add fields to type descriptor
    ret = int2dds_type_descriptor_add_u32(type_desc, "index", false);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to add index field: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_type_descriptor_add_string(type_desc, "message", 256, false);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to add message field: %d\n", ret);
        goto cleanup;
    }

    // Create Topic with type descriptor
    ret = int2dds_create_topic(participant, "HelloWorldTopic", type_desc, NULL, &topic);
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

    // Create data container
    ret = int2dds_data_create(type_desc, &data);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create data: %d\n", ret);
        goto cleanup;
    }

    // Read all available samples
    int received_count = 0;

    while (1) {
        bool valid_data = false;
        ret = int2dds_take(reader, data, &valid_data);

        if (ret == INT2DDS_RET_NO_DATA) {
            continue;
        } else if (ret != INT2DDS_RET_OK) {
            printf("Failed to take data: %d\n", ret);
            break;
        }

        if (!valid_data) continue;

        // Get field values
        uint32_t index;
        char message[256];
        size_t message_len;

        ret = int2dds_data_get_u32(data, "index", &index);
        if (ret != INT2DDS_RET_OK) {
            printf("Failed to get index: %d\n", ret);
            continue;
        }

        ret = int2dds_data_get_string(data, "message", message, sizeof(message), &message_len);
        if (ret != INT2DDS_RET_OK) {
            printf("Failed to get message: %d\n", ret);
            continue;
        }

        printf("[%d] Received: index=%u, message=\"%s\"\n",
               received_count + 1, index, message);
        received_count++;
    }

    printf("\nReceived %d messages\n", received_count);

cleanup:
    // Cleanup
    if (waitset) {
        int2dds_waitset_detach_datareader(waitset, reader);
        int2dds_waitset_delete(waitset);
    }
    if (data) int2dds_data_delete(data);
    if (reader) int2dds_delete_datareader(reader);
    if (topic) int2dds_delete_topic(topic);
    if (type_desc) int2dds_type_descriptor_delete(type_desc);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Cleanup complete\n");
    return 0;
}
