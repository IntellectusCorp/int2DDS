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

// Serialize message
size_t serialize_message(uint32_t index, const char* message, uint8_t* buffer, size_t buffer_size) {
    size_t msg_len = strlen(message);
    size_t total_size = 4 + 4 + msg_len;
    if (total_size > buffer_size) return 0;

    buffer[0] = index & 0xFF;
    buffer[1] = (index >> 8) & 0xFF;
    buffer[2] = (index >> 16) & 0xFF;
    buffer[3] = (index >> 24) & 0xFF;

    buffer[4] = msg_len & 0xFF;
    buffer[5] = (msg_len >> 8) & 0xFF;
    buffer[6] = (msg_len >> 16) & 0xFF;
    buffer[7] = (msg_len >> 24) & 0xFF;

    memcpy(buffer + 8, message, msg_len);
    return total_size;
}

// Deserialize message
int deserialize_message(const uint8_t* data, size_t size, uint32_t* index, char* message, size_t msg_buf_size) {
    if (size < 8) return -1;
    *index = data[0] | (data[1] << 8) | (data[2] << 16) | (data[3] << 24);
    uint32_t str_len = data[4] | (data[5] << 8) | (data[6] << 16) | (data[7] << 24);
    if (size < 8 + str_len) return -1;
    size_t copy_len = (str_len < msg_buf_size - 1) ? str_len : msg_buf_size - 1;
    memcpy(message, data + 8, copy_len);
    message[copy_len] = '\0';
    return 0;
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

    // Create topics
    ret = int2dds_create_topic(pub_participant, "SameDomainTopic", "SameDomainData", NULL, &pub_topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(sub_participant, "SameDomainTopic", "SameDomainData", NULL, &sub_topic);
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

    // Send and receive messages
    uint8_t buffer[512];
    int received_count = 0;
    uint32_t total_messages = 10;

    for (uint32_t i = 0; i < total_messages; i++) {
        // Send message
        char message[64];
        snprintf(message, sizeof(message), "Multi-participant message %u", i);
        size_t data_size = serialize_message(i, message, buffer, sizeof(buffer));

        ret = int2dds_write(writer, buffer, data_size);
        if (ret == INT2DDS_RET_OK) {
            printf("Sent: [%u] %s\n", i, message);
        }


        while (1) {
            size_t recv_size = 0;
            bool valid_data = false;
            ret = int2dds_take(reader, buffer, sizeof(buffer), &recv_size, &valid_data);

            if (ret == INT2DDS_RET_NO_DATA) {
                break;
            } else if (ret == INT2DDS_RET_OK && valid_data) {
                uint32_t index;
                char recv_message[256];
                if (deserialize_message(buffer, recv_size, &index, recv_message, sizeof(recv_message)) == 0) {
                    printf("Received: [%u] %s\n", index, recv_message);
                    received_count++;
                }
            }
        }
        sleep_ms(500);
    }

    printf("\nSent %u messages, received %d messages\n", total_messages, received_count);

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
