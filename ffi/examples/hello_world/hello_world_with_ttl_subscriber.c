/**
 * int2dds FFI Hello World Subscriber with multicast TTL = 64
 *
 * Mirrors `dds/examples/hello_world/hello_world_with_ttl.rs` but in C.
 * The participant is created with a PropertyQosPolicy carrying the
 * `int2dds.transport.UDPv4.multicast_ttl` property.
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

#define INT2DDS_CDR_STATIC
#include "int2dds-ffi.h"
#include "hello_world.h"

#define MULTICAST_TTL ((uint8_t)64)

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipantQos* participant_qos = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* reader_qos = NULL;
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

    printf("int2dds Hello World Subscriber (multicast TTL = %u)\n", (unsigned)MULTICAST_TTL);
    printf("Domain: %d, QoS: %s\n", domain_id, use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("----------------------------------------------------\n");

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

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
        factory, "hello_world_subscriber", domain_id, participant_qos, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(participant, "hello_world_topic", "HelloWorld",
                               1 /* APPENDABLE */, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datareader_qos_create_default(&reader_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    if (use_reliable) {
        ret = int2dds_datareader_qos_set_reliability(reader_qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datareader_qos_set_reliability(reader_qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 0);
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datareader(subscriber, topic, reader_qos, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("Subscriber ready. Waiting for publisher...\n");

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
    ret = int2dds_waitset_wait(waitset, -1);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    int32_t total_count = 0;
    int32_t current_count = 0;
    ret = int2dds_get_subscription_matched_status(reader, &total_count, &current_count);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get subscription matched status: %d\n", ret);
        goto cleanup;
    }
    printf("Publisher matched! (total: %d, current: %d)\n", total_count, current_count);
    printf("Waiting for messages...\n\n");

    uint8_t recv_buf[4096];
    uintptr_t actual_size;
    bool valid_data;
    HelloWorld hw;
    int received_count = 0;

    while (1) {
        ret = int2dds_take_serialized(reader, recv_buf, sizeof(recv_buf), &actual_size, &valid_data);

        if (ret == INT2DDS_RET_OK && valid_data) {
            if (HelloWorld_deserialize_cdr(recv_buf, actual_size, &hw)) {
                printf("[%u] Received: %s\n", hw.index, hw.message);
                received_count++;
            } else {
                fprintf(stderr, "Deserialization failed\n");
            }
        } else if (ret == INT2DDS_RET_NO_DATA) {
            int2dds_waitset_wait(waitset, 1000);
        }
    }

cleanup:
    if (waitset) {
        int2dds_waitset_detach_datareader(waitset, reader);
        int2dds_waitset_delete(waitset);
    }
    if (reader) {
        int2dds_delete_datareader(reader);
    }
    if (reader_qos) {
        int2dds_datareader_qos_destroy(reader_qos);
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
    if (participant_qos) {
        int2dds_participant_qos_destroy(participant_qos);
    }
    if (factory) {
        int2dds_domain_participant_factory_finalize(factory);
    }

    printf("\nSubscriber finished. Received %d messages.\n", received_count);
    return (ret == INT2DDS_RET_OK || ret == INT2DDS_RET_NO_DATA) ? 0 : 1;
}
