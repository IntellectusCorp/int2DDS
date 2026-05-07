/**
 * int2dds FFI Hello World Publisher with multicast TTL = 64
 *
 * Mirrors `dds/examples/hello_world/hello_world_with_ttl.rs` but in C.
 * Demonstrates how to set a per-participant IPv4 multicast TTL via the
 * PropertyQosPolicy convenience wrapper exposed by the FFI.
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

#define MULTICAST_TTL ((uint8_t)64)

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipantQos* participant_qos = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* writer_qos = NULL;
    Int2DdsWaitSet* waitset = NULL;

    int32_t domain_id = 0;
    int use_reliable = 0;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--reliable") == 0) {
            use_reliable = 1;
        } else if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        }
    }

    printf("int2dds Hello World Publisher (multicast TTL = %u)\n", (unsigned)MULTICAST_TTL);
    printf("Domain: %d, QoS: %s\n", domain_id, use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("---------------------------------------------------\n");

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Build a DomainParticipantQos with the multicast TTL property set. */
    ret = int2dds_participant_qos_create_default(&participant_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant QoS: %d\n", ret);
        goto cleanup;
    }
    ret = int2dds_participant_qos_set_multicast_ttl(participant_qos, MULTICAST_TTL);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set multicast TTL: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_participant_with_qos(
        factory, "hello_world_publisher", domain_id, participant_qos, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(participant, "hello_world_topic", "HelloWorld",
                               1 /* APPENDABLE */, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datawriter_qos_create_default(&writer_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    if (use_reliable) {
        ret = int2dds_datawriter_qos_set_reliability(writer_qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datawriter_qos_set_reliability(writer_qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 0);
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datawriter(publisher, topic, writer_qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("Publisher ready. Waiting for subscriber...\n");

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
    ret = int2dds_waitset_wait(waitset, -1);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    int32_t total_count = 0;
    int32_t current_count = 0;
    ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get publication matched status: %d\n", ret);
        goto cleanup;
    }
    printf("Subscriber matched! (total: %d, current: %d)\n", total_count, current_count);
    printf("Starting to send messages...\n\n");

    HelloWorld hw;
    uint8_t buf[4096];
    uint32_t i = 0;

    while (1) {
        i++;
        hw.index = i;
        snprintf(hw.message, sizeof(hw.message),
                 "Hello World (ttl=%u) from C! [%u]", (unsigned)MULTICAST_TTL, i);

        size_t serialized_len = HelloWorld_serialize_cdr(&hw, buf, sizeof(buf));
        if (serialized_len == 0) {
            fprintf(stderr, "Serialization failed\n");
            continue;
        }

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
        int2dds_waitset_detach_datawriter(waitset, writer);
        int2dds_waitset_delete(waitset);
    }
    if (writer) {
        int2dds_delete_datawriter(writer);
    }
    if (writer_qos) {
        int2dds_datawriter_qos_destroy(writer_qos);
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
    if (participant_qos) {
        int2dds_participant_qos_destroy(participant_qos);
    }
    if (factory) {
        int2dds_domain_participant_factory_finalize(factory);
    }

    printf("\nPublisher finished.\n");
    return (ret == INT2DDS_RET_OK) ? 0 : 1;
}
