/**
 * WaitSet Example Publisher
 *
 * Publishes messages to match with waitset_subscriber.
 * Waits for subscriber to connect before sending.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

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
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsWaitSet* waitset = NULL;
    Int2DdsTypeDescriptor* type_desc = NULL;
    Int2DdsData* data = NULL;

    int domain_id = 0;

    printf("=== int2dds WaitSet Publisher Example ===\n\n");

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

    // Create Publisher
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher: %d\n", ret);
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

    // Create Topic with type descriptor (must match waitset_subscriber)
    ret = int2dds_create_topic(participant, "HelloWorldTopic", "HelloWorldType", type_desc, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create topic: %d\n", ret);
        goto cleanup;
    }
    printf("Created topic: HelloWorldTopic\n");

    // Create DataWriter with default QoS
    ret = int2dds_create_datawriter(publisher, topic, NULL, &writer);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create data writer: %d\n", ret);
        goto cleanup;
    }
    printf("Created data writer\n\n");

    // Create WaitSet and attach writer
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datawriter(waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to attach writer to waitset: %d\n", ret);
        goto cleanup;
    }
    printf("Attached datawriter to waitset\n");

    // Wait for subscriber to match using WaitSet
    printf("Waiting for subscriber (using WaitSet)...\n");
    ret = int2dds_waitset_wait(waitset, -1);  // -1 for infinite wait
    if (ret != INT2DDS_RET_OK) {
        printf("WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    // Get matched status to confirm
    int32_t total_count = 0;
    int32_t current_count = 0;
    ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to get publication matched status: %d\n", ret);
        goto cleanup;
    }
    printf("Subscriber matched! (total: %d, current: %d)\n", total_count, current_count);
    printf("Starting to send messages...\n\n");

    // Create data container
    ret = int2dds_data_create(type_desc, &data);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create data: %d\n", ret);
        goto cleanup;
    }

    // Send messages
    char message[64];

    for (uint32_t i = 0; i < 100; i++) {
        snprintf(message, sizeof(message), "WaitSet message %u", i);

        // Set field values
        ret = int2dds_data_set_u32(data, "index", i);
        if (ret != INT2DDS_RET_OK) {
            printf("Failed to set index: %d\n", ret);
            continue;
        }

        ret = int2dds_data_set_string(data, "message", message);
        if (ret != INT2DDS_RET_OK) {
            printf("Failed to set message: %d\n", ret);
            continue;
        }

        // Write data (automatic CDR serialization)
        ret = int2dds_write(writer, data);
        if (ret != INT2DDS_RET_OK) {
            printf("Failed to write: %d\n", ret);
        } else {
            printf("[%u] Sent: %s\n", i, message);
        }

        sleep_ms(1000);
    }

    printf("\nPublishing complete\n");

cleanup:
    if (waitset) {
        int2dds_waitset_detach_datawriter(waitset, writer);
        int2dds_waitset_delete(waitset);
    }
    if (data) int2dds_data_delete(data);
    if (writer) int2dds_delete_datawriter(writer);
    if (topic) int2dds_delete_topic(topic);
    if (type_desc) int2dds_type_descriptor_delete(type_desc);
    if (publisher) int2dds_delete_publisher(publisher);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Cleanup complete\n");
    return 0;
}
