/**
 * int2dds FFI Hello World Publisher Example
 *
 * This example demonstrates how to use the int2dds FFI with IDL-generated
 * code for CDR serialization.
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

#define INT2DDS_CDR_STATIC
#include "int2dds-ffi.h"
#include "hello_world.h"

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;
    Int2DdsStatusCondition* condition = NULL;

    int32_t domain_id = 0;
    int use_reliable = 0;  /* 0 = best effort, 1 = reliable */

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--reliable") == 0) {
            use_reliable = 1;
        } else if ((strcmp(argv[i], "--domain") == 0 || strcmp(argv[i], "-d") == 0)
                   && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        }
    }

    printf("int2dds IDL Hello World Publisher\n");
    printf("Domain: %d, QoS: %s\n", domain_id, use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("-----------------------------------\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Create participant */
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

    /* Create topic with extensibility (1 = APPENDABLE) */
    ret = int2dds_create_topic(participant, "hello_world_topic", "HelloWorld",
                               1,  /* APPENDABLE (default extensibility) */
                               NULL, &topic);
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
        ret = int2dds_datawriter_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
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

    /* Create WaitSet */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    /* Restrict to PUBLICATION_MATCHED; the default mask (ALL) would also wake on OFFERED_INCOMPATIBLE_QOS */
    ret = int2dds_datawriter_get_statuscondition(writer, &condition);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get writer status condition: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_statuscondition_set_enabled_statuses(condition, INT2DDS_STATUS_PUBLICATION_MATCHED);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set enabled statuses: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_condition(waitset, condition);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach condition to waitset: %d\n", ret);
        goto cleanup;
    }

    /* Loop until a subscriber actually matches; the wait may also return on unmatch */
    int32_t total_count = 0;
    int32_t current_count = 0;
    do {
        ret = int2dds_waitset_wait(waitset, -1);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "WaitSet wait failed: %d\n", ret);
            goto cleanup;
        }

        ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to get publication matched status: %d\n", ret);
            goto cleanup;
        }
    } while (current_count <= 0);

    printf("Subscriber matched! (total: %d, current: %d)\n", total_count, current_count);
    printf("Starting to send messages...\n\n");

    /* Publish messages using IDL-generated serialization */
    HelloWorld hw;
    uint8_t buf[4096];
    uint32_t i = 0;

    while (1) {
        i++;

        /* Fill struct directly */
        hw.index = i;
        snprintf(hw.message, sizeof(hw.message), "Hello World from C! [%u]", i);

        /* Serialize to CDR bytes */
        size_t serialized_len = HelloWorld_serialize_cdr(&hw, buf, sizeof(buf), false);
        if (serialized_len == 0) {
            fprintf(stderr, "Serialization failed\n");
            continue;
        }

        /* Write serialized bytes (no key for HelloWorld) */
        ret = int2dds_write_serialized(writer, buf, serialized_len, NULL, 0);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to write: %d\n", ret);
        } else {
            printf("[%u] Sent: %s\n", i, hw.message);
        }

        sleep_ms(1000);
    }

cleanup:
    if (waitset) {
        if (condition) {
            int2dds_waitset_detach_condition(waitset, condition);
        }
        int2dds_waitset_delete(waitset);
    }
    if (condition) {
        int2dds_statuscondition_delete(condition);
    }
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
