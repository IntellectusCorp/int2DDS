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


int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;
    Int2DdsTypeDescriptor* type_desc = NULL;
    Int2DdsData* data = NULL;

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

    /* Create type descriptor for HelloWorld */
    ret = int2dds_type_descriptor_create("HelloWorld", &type_desc);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create type descriptor: %d\n", ret);
        goto cleanup;
    }

    /* Add fields to type descriptor */
    ret = int2dds_type_descriptor_add_u32(type_desc, "index", false);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to add index field: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_type_descriptor_add_string(type_desc, "message", 256, false);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to add message field: %d\n", ret);
        goto cleanup;
    }

    /* Create topic with type descriptor */
    ret = int2dds_create_topic(participant, "hello_world_topic", type_desc, NULL, &topic);
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

    /* Create data container */
    ret = int2dds_data_create(type_desc, &data);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create data: %d\n", ret);
        goto cleanup;
    }

    /* Receive messages */
    bool valid_data;
    uint32_t index;
    char message[256];
    size_t message_len;
    int received_count = 0;

    while (received_count < 100) {
        ret = int2dds_take(reader, data, &valid_data);

        if (ret == INT2DDS_RET_OK && valid_data) {
            /* Get field values */
            ret = int2dds_data_get_u32(data, "index", &index);
            if (ret != INT2DDS_RET_OK) {
                fprintf(stderr, "Failed to get index: %d\n", ret);
                continue;
            }

            ret = int2dds_data_get_string(data, "message", message, sizeof(message), &message_len);
            if (ret != INT2DDS_RET_OK) {
                fprintf(stderr, "Failed to get message: %d\n", ret);
                continue;
            }

            printf("[%u] Received: %s\n", index, message);
            received_count++;
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
    if (data) {
        int2dds_data_delete(data);
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
    if (type_desc) {
        int2dds_type_descriptor_delete(type_desc);
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
