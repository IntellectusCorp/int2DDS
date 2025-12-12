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

// Serialize HelloWorld: index (u32) + message (String with length prefix)
size_t serialize_hello_world(uint32_t index, const char* message, uint8_t* buffer, size_t buffer_size) {
    size_t msg_len = strlen(message);
    size_t total_size = 4 + 4 + msg_len;  // index + string_len + string

    if (total_size > buffer_size) return 0;

    // Write index (little-endian u32)
    buffer[0] = index & 0xFF;
    buffer[1] = (index >> 8) & 0xFF;
    buffer[2] = (index >> 16) & 0xFF;
    buffer[3] = (index >> 24) & 0xFF;

    // Write string length (little-endian u32)
    buffer[4] = msg_len & 0xFF;
    buffer[5] = (msg_len >> 8) & 0xFF;
    buffer[6] = (msg_len >> 16) & 0xFF;
    buffer[7] = (msg_len >> 24) & 0xFF;

    // Write string data
    memcpy(buffer + 8, message, msg_len);

    return total_size;
}

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsWaitSet* waitset = NULL;

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

    // Create Topic (must match waitset_subscriber)
    ret = int2dds_create_topic(participant, "HelloWorldTopic", "HelloWorld", NULL, &topic);
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

    // Send messages
    uint8_t buffer[512];

    for (uint32_t i = 0; i < 100; i++) {
        char message[64];
        snprintf(message, sizeof(message), "WaitSet message %u", i);

        size_t data_size = serialize_hello_world(i, message, buffer, sizeof(buffer));
        if (data_size == 0) {
            printf("Failed to serialize\n");
            continue;
        }

        ret = int2dds_write(writer, buffer, data_size);
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
    if (writer) int2dds_delete_datawriter(writer);
    if (topic) int2dds_delete_topic(topic);
    if (publisher) int2dds_delete_publisher(publisher);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Cleanup complete\n");
    return 0;
}
