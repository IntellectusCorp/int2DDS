/**
 * int2dds FFI QoS Profile Publisher Example
 *
 * Loads QoS settings from an XML profile file (OMG <qos_library> syntax)
 * and creates the entities with the named profile. The profile is loaded by the
 * factory from the DDS_QOS_PROFILE environment variable; this example sets that
 * variable to the bundled qos_profiles.xml before the factory is created (so a
 * preset DDS_QOS_PROFILE in the environment wins, and you can also override the
 * file with --qos-profile PATH).
 *
 * Build/run (from ffi/examples/build):
 *   ./qos_profile_publisher [--domain N] [--qos-profile PATH]
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

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
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
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

    /* Point the factory at the XML profile file. A DDS_QOS_PROFILE already set
     * in the environment takes precedence and is left untouched. */
    if (getenv("DDS_QOS_PROFILE") == NULL) {
        set_env("DDS_QOS_PROFILE", qos_file ? qos_file : DEFAULT_QOS_FILE);
    }

    printf("int2dds QoS Profile Publisher\n");
    printf("Domain: %d\n", domain_id);
    printf("DDS_QOS_PROFILE: %s\n", getenv("DDS_QOS_PROFILE"));
    printf("-----------------------------------\n");

    /* Initialize factory (auto-loads profiles from DDS_QOS_PROFILE) */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Entities are created with default QoS (NULL handle). The factory resolves
     * default QoS to the profile marked is_default_profile="true" in the XML,
     * falling back to spec defaults for any policy the profile does not set. */
    ret = int2dds_create_participant(factory, "qos_profile_publisher", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    /* HelloWorld is APPENDABLE (1) and non-keyed (NULL key descriptor) */
    ret = int2dds_create_topic(participant, "hello_world_topic", "HelloWorld",
                               1, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datawriter(publisher, topic, NULL, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    /* Report the QoS that the profile produced */
    {
        Int2DdsDataWriterQos* eff_qos = NULL;
        if (int2dds_datawriter_get_qos(writer, &eff_qos) == INT2DDS_RET_OK) {
            int32_t rel = INT2DDS_QOS_RELIABILITY_BEST_EFFORT;
            int64_t max_block = 0;
            int32_t dur = INT2DDS_QOS_DURABILITY_VOLATILE;
            int32_t hist = INT2DDS_QOS_HISTORY_KEEP_LAST;
            int32_t depth = 0;
            int2dds_datawriter_qos_get_reliability(eff_qos, &rel, &max_block);
            int2dds_datawriter_qos_get_durability(eff_qos, &dur);
            int2dds_datawriter_qos_get_history(eff_qos, &hist, &depth);
            printf("DataWriter QoS in effect: reliability=%d durability=%d history=%d depth=%d\n",
                   rel, dur, hist, depth);
            int2dds_datawriter_qos_destroy(eff_qos);
        }
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

    {
        int32_t total_count = 0, current_count = 0;
        int2dds_get_publication_matched_status(writer, &total_count, &current_count);
        printf("Subscriber matched! (total: %d, current: %d)\n", total_count, current_count);
        printf("Starting to send messages...\n\n");
    }

    HelloWorld hw;
    uint8_t buf[4096];
    uint32_t i = 0;

    while (1) {
        i++;
        hw.index = i;
        snprintf(hw.message, sizeof(hw.message), "Hello from QoS profile (C)! [%u]", i);

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
