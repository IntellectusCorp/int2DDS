/**
 * int2dds FFI Hello World Subscriber Example
 *
 * This example demonstrates how to use the int2dds FFI to subscribe to messages.
 * It receives HelloWorld messages containing an index and a message string.
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

#include "int2dds-ffi.h"

/* Simple HelloWorld data structure */
typedef struct {
    uint32_t index;
    char message[256];
} HelloWorld;

/* Deserialize raw bytes to HelloWorld */
static int deserialize_hello_world(const uint8_t* buffer, size_t size, HelloWorld* data) {
    if (size < sizeof(uint32_t) + 1) {
        return -1;
    }

    /* Read index (little-endian) */
    data->index = (uint32_t)buffer[0] |
                  ((uint32_t)buffer[1] << 8) |
                  ((uint32_t)buffer[2] << 16) |
                  ((uint32_t)buffer[3] << 24);

    /* Read message */
    size_t msg_len = size - sizeof(uint32_t);
    if (msg_len >= sizeof(data->message)) {
        msg_len = sizeof(data->message) - 1;
    }
    memcpy(data->message, buffer + 4, msg_len);
    data->message[msg_len] = '\0';

    return 0;
}

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;

    int32_t domain_id = 0;
    int use_reliable = 0;  /* 0 = best effort, 1 = reliable */

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--reliable") == 0) {
            use_reliable = 1;
        } else if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        }
    }

    printf("int2dds FFI Hello World Subscriber\n");
    printf("Domain: %d, QoS: %s\n", domain_id, use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("------------------------------------\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Create participant (DomainParticipant) */
    ret = int2dds_create_participant(factory, "hello_world_subscriber", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create subscriber */
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    /* Create topic */
    ret = int2dds_create_topic(participant, "HelloWorld", "HelloWorld", NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* Create DataReader QoS */
    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    /* Set reliability */
    if (use_reliable) {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE);
    } else {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT);
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    /* Create DataReader */
    ret = int2dds_create_datareader(subscriber, topic, qos, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Subscriber ready. Waiting for publisher...\n");

    /* Create WaitSet and attach reader */
    Int2DdsWaitSet* waitset = NULL;
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datareader(waitset, reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach reader to waitset: %d\n", ret);
        goto cleanup;
    }

    /* Wait for publisher to match using WaitSet */
    ret = int2dds_waitset_wait(waitset, -1);  /* -1 for infinite wait */
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    /* Get matched status to confirm */
    int32_t total_count = 0;
    int32_t current_count = 0;
    ret = int2dds_get_subscription_matched_status(reader, &total_count, &current_count);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get subscription matched status: %d\n", ret);
        goto cleanup;
    }
    printf("Publisher matched! (total: %d, current: %d)\n", total_count, current_count);
    printf("Waiting for messages...\n\n");

    /* Receive messages */
    uint8_t buffer[512];
    size_t data_size;
    bool valid_data;
    HelloWorld data;
    int received_count = 0;

    while (received_count < 100) {
        ret = int2dds_take(reader, buffer, sizeof(buffer), &data_size, &valid_data);

        if (ret == INT2DDS_RET_OK && valid_data) {
            if (deserialize_hello_world(buffer, data_size, &data) == 0) {
                printf("[%u] Received: %s\n", data.index, data.message);
                received_count++;
            } else {
                fprintf(stderr, "Failed to deserialize data\n");
            }
        } else if (ret == INT2DDS_RET_NO_DATA) {
            /* No data available, wait and try again */
            sleep_ms(100);
        } else if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to take: %d\n", ret);
            sleep_ms(100);
        }
    }

cleanup:
    /* Cleanup in reverse order */
    if (waitset) {
        int2dds_waitset_detach_datareader(waitset, reader);
        int2dds_waitset_delete(waitset);
    }
    if (reader) {
        int2dds_delete_datareader(reader);
    }
    if (qos) {
        int2dds_datareader_qos_destroy(qos);
    }
    if (topic) {
        int2dds_delete_topic(topic);
    }
    if (subscriber) {
        int2dds_delete_subscriber(subscriber);
    }
    if (participant) {
        int2dds_delete_participant(participant);
    }
    if (factory) {
        int2dds_domain_participant_factory_finalize(factory);
    }

    printf("\nSubscriber finished. Received %d messages.\n", received_count);
    return (ret == INT2DDS_RET_OK || ret == INT2DDS_RET_NO_DATA) ? 0 : 1;
}
