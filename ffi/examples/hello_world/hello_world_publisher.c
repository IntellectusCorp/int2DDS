/**
 * int2dds FFI Hello World Publisher Example
 *
 * This example demonstrates how to use the int2dds FFI to publish messages.
 * It publishes simple HelloWorld messages containing an index and a message string.
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

#include "int2dds-ffi.h"

/* Simple HelloWorld data structure */
typedef struct {
    uint32_t index;
    char message[256];
} HelloWorld;

/* Serialize HelloWorld to raw bytes (simple format: 4 bytes index + null-terminated string) */
static size_t serialize_hello_world(const HelloWorld* data, uint8_t* buffer, size_t buffer_size) {
    size_t msg_len = strlen(data->message) + 1;  /* Include null terminator */
    size_t total_size = sizeof(uint32_t) + msg_len;

    if (buffer_size < total_size) {
        return 0;
    }

    /* Write index (little-endian) */
    buffer[0] = (uint8_t)(data->index & 0xFF);
    buffer[1] = (uint8_t)((data->index >> 8) & 0xFF);
    buffer[2] = (uint8_t)((data->index >> 16) & 0xFF);
    buffer[3] = (uint8_t)((data->index >> 24) & 0xFF);

    /* Write message */
    memcpy(buffer + 4, data->message, msg_len);

    return total_size;
}

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;

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

    printf("int2dds FFI Hello World Publisher\n");
    printf("Domain: %d, QoS: %s\n", domain_id, use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("-----------------------------------\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Create participant (DomainParticipant) */
    ret = int2dds_create_participant(factory, "hello_world_publisher", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create publisher */
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    /* Create topic */
    ret = int2dds_create_topic(participant, "HelloWorld", "HelloWorld", NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* Create DataWriter QoS */
    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    /* Set reliability */
    if (use_reliable) {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);  /* 100ms */
    } else {
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 0);
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    /* Create DataWriter */
    ret = int2dds_create_datawriter(publisher, topic, qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("Publisher ready. Waiting for subscriber...\n");

    /* Create WaitSet and attach writer */
    Int2DdsWaitSet* waitset = NULL;
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datawriter(waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach writer to waitset: %d\n", ret);
        goto cleanup;
    }

    /* Wait for subscriber to match using WaitSet */
    ret = int2dds_waitset_wait(waitset, -1);  /* -1 for infinite wait */
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    /* Get matched status to confirm */
    int32_t total_count = 0;
    int32_t current_count = 0;
    ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get publication matched status: %d\n", ret);
        goto cleanup;
    }
    printf("Subscriber matched! (total: %d, current: %d)\n", total_count, current_count);
    printf("Starting to send messages...\n\n");

    /* Publish messages */
    uint8_t buffer[512];
    HelloWorld data;

    for (uint32_t i = 0; i < 100; i++) {
        data.index = i;
        snprintf(data.message, sizeof(data.message), "Hello World from C! [%u]", i);

        size_t serialized_size = serialize_hello_world(&data, buffer, sizeof(buffer));
        if (serialized_size == 0) {
            fprintf(stderr, "Failed to serialize data\n");
            continue;
        }

        ret = int2dds_write(writer, buffer, serialized_size);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to write: %d\n", ret);
        } else {
            printf("[%u] Sent: %s\n", data.index, data.message);
        }

        sleep_ms(1000);  /* 1 second delay */
    }

    if (waitset) {
        int2dds_waitset_detach_datawriter(waitset, writer);
        int2dds_waitset_delete(waitset);
    }

cleanup:
    /* Cleanup in reverse order */
    if (writer) {
        int2dds_delete_datawriter(writer);
    }
    if (qos) {
        int2dds_datawriter_qos_destroy(qos);
    }
    if (topic) {
        int2dds_delete_topic(topic);
    }
    if (publisher) {
        int2dds_delete_publisher(publisher);
    }
    if (participant) {
        int2dds_delete_participant(participant);
    }
    if (factory) {
        int2dds_domain_participant_factory_finalize(factory);
    }

    printf("\nPublisher finished.\n");
    return (ret == INT2DDS_RET_OK) ? 0 : 1;
}
