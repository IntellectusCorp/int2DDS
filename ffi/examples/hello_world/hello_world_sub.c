/**
 * int2dds FFI Hello World Subscriber Example
 *
 * This example demonstrates how to use the int2dds FFI with IDL-generated
 * code for CDR deserialization.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
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
    /* Unbuffer stdout so redirected output (CI logs) appears as it is produced.
       _IONBF rather than _IOLBF because the MSVC CRT does not implement line buffering. */
    setvbuf(stdout, NULL, _IONBF, 0);

    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* qos = NULL;
    Int2DdsWaitSet* waitset = NULL;
    Int2DdsStatusCondition* condition = NULL;

    int32_t domain_id = 0;
    int use_reliable = 0;
    const char* topic_name = "hello_world_topic";

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

    /* Create subscriber */
    ret = int2dds_create_subscriber(participant, NULL, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    /* Create topic — extensibility is carried by the type via HelloWorld_type_info(),
       so no extensibility argument is passed (parity with the Rust/Python/C# examples). */
    ret = HelloWorld_create_topic(participant, topic_name, NULL, &topic);
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
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 0);
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    /* Create DataReader (no listener) */
    ret = int2dds_create_datareader(subscriber, topic, qos, NULL, 0, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    int32_t rel_kind = 0, dur_kind = 0, hist_kind = 0, hist_depth = 0;
    int64_t max_blocking_ns = 0;
    char hist_text[32];
    int2dds_datareader_qos_get_reliability(qos, &rel_kind, &max_blocking_ns);
    int2dds_datareader_qos_get_durability(qos, &dur_kind);
    int2dds_datareader_qos_get_history(qos, &hist_kind, &hist_depth);
    history_name(hist_kind, hist_depth, hist_text, sizeof(hist_text));

    printf("[subscriber INFO] domain_id: %d, topic: %s\n", domain_id, topic_name);
    printf("[subscriber qos] reliability: %s, durability: %s, history: %s\n",
           reliability_name(rel_kind), durability_name(dur_kind), hist_text);

    /* Create WaitSet */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    /* SUBSCRIPTION_MATCHED for matching below, DATA_AVAILABLE for the receive loop */
    ret = int2dds_datareader_get_statuscondition(reader, &condition);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get reader status condition: %d\n", ret);
        goto cleanup;
    }

    /* Default mask (ALL) would also wake on REQUESTED_INCOMPATIBLE_QOS, so restrict it */
    ret = int2dds_statuscondition_set_enabled_statuses(
        condition, INT2DDS_STATUS_SUBSCRIPTION_MATCHED | INT2DDS_STATUS_DATA_AVAILABLE);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set enabled statuses: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_statuscondition(waitset, condition);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach condition to waitset: %d\n", ret);
        goto cleanup;
    }

    /* Loop until a publisher actually matches; the finite timeout keeps Ctrl-C responsive */
    struct Int2DdsSubscriptionMatchedStatus matched = {0};
    do {
        struct Int2DdsConditionSeq *triggered = NULL;
        ret = int2dds_waitset_wait_ex(waitset, 1000, &triggered);
        int2dds_condition_seq_delete(triggered);
        if (ret != INT2DDS_RET_OK && ret != INT2DDS_RET_TIMEOUT) {
            fprintf(stderr, "WaitSet wait failed: %d\n", ret);
            goto cleanup;
        }
        if (g_stop) {
            ret = INT2DDS_RET_OK;
            goto cleanup;
        }

        ret = int2dds_datareader_get_subscription_matched_status(reader, &matched);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to get subscription matched status: %d\n", ret);
            goto cleanup;
        }
    } while (matched.current_count <= 0);

    printf("Publisher matched!\n");

    /* Receive messages using IDL-generated deserialization */
    uint8_t recv_buf[4096];
    uintptr_t actual_size;
    bool valid_data;
    HelloWorld hw;
    int received_count = 0;

    while (!g_stop) {
        ret = int2dds_datareader_take_serialized(reader, recv_buf, sizeof(recv_buf), &actual_size, &valid_data);

        if (ret == INT2DDS_RET_OK && valid_data) {
            /* Deserialize CDR bytes to HelloWorld struct */
            if (HelloWorld_deserialize_cdr(recv_buf, actual_size, &hw)) {
                printf("Read sample: HelloWorld { index: %u, message: \"%s\" }\n", hw.index, hw.message);
                received_count++;
            } else {
                fprintf(stderr, "Deserialization failed\n");
            }
        } else if (ret == INT2DDS_RET_NO_DATA) {
            /* No data yet - wait for data available */
            struct Int2DdsConditionSeq *triggered = NULL;
            int2dds_waitset_wait_ex(waitset, 1000, &triggered);
            int2dds_condition_seq_delete(triggered);
        }
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
