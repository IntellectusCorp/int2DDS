/**
 * int2dds FFI QoS Profile Subscriber Example
 *
 * Companion to qos_profile_publisher. Loads QoS settings from an XML profile
 * file (RTI/OMG <qos_library> syntax) and creates the entities with the named
 * profile. The profile is loaded by the factory from the DDS_QOS_PROFILE
 * environment variable; this example sets that variable to the bundled
 * qos_profiles.xml before the factory is created (a preset DDS_QOS_PROFILE in
 * the environment wins, and --qos-profile PATH overrides the file).
 *
 * Build/run (from ffi/examples/build):
 *   ./qos_profile_subscriber [--domain N] [--qos-profile PATH]
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef _WIN32
#include <windows.h>
#define sleep_ms(ms) Sleep(ms)
#define set_env(name, value) _putenv_s(name, value)
#else
#include <unistd.h>
#define sleep_ms(ms) usleep((ms) * 1000)
#define set_env(name, value) setenv(name, value, 1)
#endif

#define INT2DDS_CDR_STATIC
#include "int2dds-ffi.h"
#include "../hello_world/hello_world.h"

#define DEFAULT_QOS_FILE "../qos_profile/qos_profiles.xml"

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsWaitSet* waitset = NULL;

    int32_t domain_id = 0;
    const char* qos_file = NULL;

    setvbuf(stdout, NULL, _IONBF, 0);

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--qos-profile") == 0 && i + 1 < argc) {
            qos_file = argv[++i];
        }
    }

    if (getenv("DDS_QOS_PROFILE") == NULL) {
        set_env("DDS_QOS_PROFILE", qos_file ? qos_file : DEFAULT_QOS_FILE);
    }

    printf("int2dds QoS Profile Subscriber\n");
    printf("Domain: %d\n", domain_id);
    printf("DDS_QOS_PROFILE: %s\n", getenv("DDS_QOS_PROFILE"));
    printf("------------------------------------\n");

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Entities are created with default QoS (NULL handle). The factory resolves
     * default QoS to the profile marked is_default_profile="true" in the XML,
     * falling back to spec defaults for any policy the profile does not set. */
    ret = int2dds_create_participant(factory, "qos_profile_subscriber", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    /* HelloWorld is APPENDABLE (1) and non-keyed (NULL key descriptor) */
    ret = int2dds_create_topic(participant, "hello_world_topic", "HelloWorld",
                               1, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datareader(subscriber, topic, NULL, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    {
        Int2DdsDataReaderQos* eff_qos = NULL;
        if (int2dds_datareader_get_qos(reader, &eff_qos) == INT2DDS_RET_OK) {
            int32_t rel = INT2DDS_QOS_RELIABILITY_BEST_EFFORT;
            int64_t max_block = 0;
            int32_t dur = INT2DDS_QOS_DURABILITY_VOLATILE;
            int32_t hist = INT2DDS_QOS_HISTORY_KEEP_LAST;
            int32_t depth = 0;
            int2dds_datareader_qos_get_reliability(eff_qos, &rel, &max_block);
            int2dds_datareader_qos_get_durability(eff_qos, &dur);
            int2dds_datareader_qos_get_history(eff_qos, &hist, &depth);
            printf("DataReader QoS in effect: reliability=%d durability=%d history=%d depth=%d\n",
                   rel, dur, hist, depth);
            int2dds_datareader_qos_destroy(eff_qos);
        }
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

    {
        int32_t total_count = 0, current_count = 0;
        int2dds_get_subscription_matched_status(reader, &total_count, &current_count);
        printf("Publisher matched! (total: %d, current: %d)\n", total_count, current_count);
        printf("Waiting for messages...\n\n");
    }

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
