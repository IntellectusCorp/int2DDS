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
#include <signal.h>

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

/* Run until Ctrl-C, then clean up gracefully (matches the Rust example). */
static volatile sig_atomic_t g_stop = 0;
static void handle_sigint(int sig) {
    (void)sig;
    g_stop = 1;
}

const char* topic_name = "hello_world_topic";

static const char* reliability_name(int32_t kind) {
    return kind == INT2DDS_QOS_RELIABILITY_RELIABLE ? "Reliable" : "BestEffort";
}

static const char* durability_name(int32_t kind) {
    switch (kind) {
        case INT2DDS_QOS_DURABILITY_TRANSIENT_LOCAL: return "TransientLocal";
        case INT2DDS_QOS_DURABILITY_TRANSIENT:       return "Transient";
        case INT2DDS_QOS_DURABILITY_PERSISTENT:      return "Persistent";
        default:                                     return "Volatile";
    }
}

static void history_name(int32_t kind, int32_t depth, char* out, size_t out_len) {
    if (kind == INT2DDS_QOS_HISTORY_KEEP_ALL) {
        snprintf(out, out_len, "KeepAll");
    } else {
        snprintf(out, out_len, "KeepLast(%d)", depth);
    }
}

int main(int argc, char* argv[]) {
    signal(SIGINT, handle_sigint);

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

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, domain_id, NULL, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* Create publisher */
    ret = int2dds_create_publisher(participant, NULL, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    /* Create topic — extensibility is carried by the type via HelloWorld_type_info(),
       so no extensibility argument is passed (parity with the Rust/Python/C# examples). */
    ret = HelloWorld_create_topic(participant, topic_name, NULL, &topic);
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

    /* Create DataWriter (no listener) */
    ret = int2dds_create_datawriter(publisher, topic, qos, NULL, 0, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    int32_t rel_kind = 0, dur_kind = 0, hist_kind = 0, hist_depth = 0;
    int64_t max_blocking_ns = 0;
    char hist_text[32];
    int2dds_datawriter_qos_get_reliability(qos, &rel_kind, &max_blocking_ns);
    int2dds_datawriter_qos_get_durability(qos, &dur_kind);
    int2dds_datawriter_qos_get_history(qos, &hist_kind, &hist_depth);
    history_name(hist_kind, hist_depth, hist_text, sizeof(hist_text));

    printf("[publisher INFO] domain_id: %d, topic: %s\n", domain_id, topic_name);
    printf("[publisher qos] reliability: %s, durability: %s, history: %s\n",
           reliability_name(rel_kind), durability_name(dur_kind), hist_text);

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

    ret = int2dds_waitset_attach_statuscondition(waitset, condition);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach condition to waitset: %d\n", ret);
        goto cleanup;
    }

    /* Loop until a subscriber actually matches; the finite timeout keeps Ctrl-C responsive */
    struct Int2DdsPublicationMatchedStatus matched = {0};
    do {
        struct Int2DdsConditionSeq *triggered = NULL;
        ret = int2dds_waitset_wait_ex(waitset, -1, &triggered);
        int2dds_condition_seq_delete(triggered);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "WaitSet wait failed: %d\n", ret);
            goto cleanup;
        }
        if (g_stop) {
            goto cleanup;
        }

        ret = int2dds_datawriter_get_publication_matched_status(writer, &matched);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to get publication matched status: %d\n", ret);
            goto cleanup;
        }
    } while (matched.current_count <= 0);

    printf("Subscriber matched!\n");

    /* Publish messages using IDL-generated serialization */
    HelloWorld hw;
    uint8_t buf[4096];
    uint32_t i = 0;

    while (!g_stop) {
        i++;

        /* Fill struct directly */
        hw.index = i;
        snprintf(hw.message, sizeof(hw.message), "[C]HelloWorld_d%d", domain_id);

        /* Serialize to CDR bytes */
        size_t serialized_len = HelloWorld_serialize_cdr(&hw, buf, sizeof(buf), false);
        if (serialized_len == 0) {
            fprintf(stderr, "Serialization failed\n");
            continue;
        }

        /* Write serialized bytes (no key for HelloWorld) */
        ret = int2dds_datawriter_write_serialized(writer, buf, serialized_len, NULL, 0);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to write: %d\n", ret);
        } else {
            printf("Published HelloWorld { index: %u, message: \"%s\" }\n", hw.index, hw.message);
        }

        sleep_ms(1000);
    }

cleanup:
    if (waitset) {
        if (condition) {
            int2dds_waitset_detach_statuscondition(waitset, condition);
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
