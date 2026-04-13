/**
 * int2dds FFI Listener Example - Subscriber
 *
 * This example demonstrates how to use listener callbacks for DataReader.
 * It shows:
 * - on_data_available: Called when new data arrives
 * - on_subscription_matched: Called when publishers are discovered/lost
 * - Using user context to pass application state to callbacks
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
#include "int2dds_cdr.h"

/* HelloWorld type - same layout as hello_world.h */
typedef struct HelloWorld {
    uint32_t index;
    char message[257];
} HelloWorld;

static bool HelloWorld_deserialize_cdr(const uint8_t *buf, size_t len, HelloWorld *val_out) {
    Int2DdsCdrReader r;
    if (int2dds_cdr_reader_init(&r, buf, len) != INT2DDS_CDR_OK)
        return false;
    uint32_t obj_size = 0;
    size_t start_pos = 0;
    int2dds_cdr_read_dheader(&r, &obj_size, &start_pos);
    int2dds_cdr_read_u32(&r, &val_out->index);
    int2dds_cdr_read_string_copy(&r, val_out->message, 257, NULL);
    int2dds_cdr_read_dheader_end(&r, obj_size, start_pos);
    return int2dds_cdr_reader_error(&r) == INT2DDS_CDR_OK;
}

/* Application context - passed to callbacks via user_context */
typedef struct {
    int message_count;
} AppContext;

/**
 * Callback: Invoked when new data is available
 * This is called from a DDS background thread - must be thread-safe!
 */
void on_data_available(Int2DdsDataReader* reader, void* user_ctx) {
    AppContext* ctx = (AppContext*)user_ctx;
    Int2DdsRet ret;

    printf("\n[Callback] on_data_available called!\n");

    /* Take all available data */
    uint8_t recv_buf[4096];
    uintptr_t actual_size;
    bool valid_data;
    HelloWorld hw;

    while ((ret = int2dds_take_serialized(reader, recv_buf, sizeof(recv_buf),
                                          &actual_size, &valid_data)) == INT2DDS_RET_OK) {
        if (valid_data) {
            if (HelloWorld_deserialize_cdr(recv_buf, actual_size, &hw)) {
                printf("  [Data] Received: index=%u, message='%s'\n", hw.index, hw.message);
                ctx->message_count++;
            } else {
                fprintf(stderr, "  [Error] Deserialization failed\n");
            }
        } else {
            printf("  [Info] Received invalid sample (metadata only)\n");
        }
    }

    printf("  Total messages received so far: %d\n", ctx->message_count);
}

/**
 * Callback: Invoked when publishers are discovered or lost
 * This is called from a DDS background thread - must be thread-safe!
 */
void on_subscription_matched(Int2DdsDataReader* reader,
                             const Int2DdsSubscriptionMatchedStatus* status,
                             void* user_ctx) {
    (void)reader;
    (void)user_ctx;

    printf("\n[Callback] Subscription Matched Event:\n");
    printf("  Current publishers: %d\n", status->current_count);
    printf("  Total matched: %d\n", status->total_count);
    printf("  Change: %d (%s)\n",
           status->current_count_change,
           status->current_count_change > 0 ? "publisher joined" : "publisher left");
    printf("  Last publication handle: ");
    for (int i = 0; i < 16; i++) {
        printf("%02x", status->last_publication_handle[i]);
    }
    printf("\n\n");
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
    int use_reliable = 1;  /* Use reliable for better demonstration */
    int wait_time = 30;    /* Wait 30 seconds for data */

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--best-effort") == 0) {
            use_reliable = 0;
        } else if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--wait") == 0 && i + 1 < argc) {
            wait_time = atoi(argv[++i]);
        }
    }

    printf("=== int2dds FFI Listener Subscriber ===\n");
    printf("Domain: %d, QoS: %s\n", domain_id, use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("Topic: HelloWorldTopic\n");
    printf("Wait time: %d seconds\n", wait_time);
    printf("=======================================\n\n");
    printf("This example demonstrates DataReader listener callbacks.\n");
    printf("Start the publisher to see callbacks fire automatically!\n\n");

    /* Initialize application context */
    AppContext app_context = {0};

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, "listener_subscriber", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        return 1;
    }

    /* Create subscriber */
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup_participant;
    }

    /* Create topic (Appendable extensibility) */
    ret = int2dds_create_topic(participant, "HelloWorldTopic", "HelloWorld",
                               1,  /* APPENDABLE */
                               NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup_subscriber;
    }

    /* Configure QoS */
    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup_topic;
    }

    if (use_reliable) {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    } else {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT, 0);
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability QoS: %d\n", ret);
        goto cleanup_qos;
    }

    /* Configure listener with callbacks and user context */
    {
        Int2DdsDataReaderListener listener = {0};
        listener.on_data_available = on_data_available;
        listener.on_subscription_matched = on_subscription_matched;
        listener.on_sample_rejected = NULL;
        listener.on_liveliness_changed = NULL;
        listener.on_requested_deadline_missed = NULL;
        listener.on_requested_incompatible_qos = NULL;
        listener.on_sample_lost = NULL;
        listener.user_context = &app_context;

        printf("Creating DataReader with listener...\n");

        /* Create reader with listener - ALL status changes enabled (0xFFFFFFFF) */
        ret = int2dds_create_datareader_with_listener(
            subscriber,
            topic,
            qos,
            &listener,
            0xFFFFFFFF,
            &reader
        );
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader with listener: %d\n", ret);
        goto cleanup_qos;
    }

    printf("DataReader created successfully!\n");
    printf("Waiting for data (callbacks will be invoked automatically)...\n\n");

    /* Wait for data - callbacks handle everything! */
    for (int i = 0; i < wait_time; i++) {
        sleep_ms(1000);
    }

    printf("\n=== Final Statistics ===\n");
    printf("Total messages received: %d\n", app_context.message_count);
    printf("========================\n\n");

    /* Cleanup */
    if (reader) {
        printf("Cleaning up...\n");
        int2dds_delete_datareader(reader);
    }

cleanup_qos:
    if (qos) int2dds_datareader_qos_destroy(qos);

cleanup_topic:
    if (topic) int2dds_delete_topic(topic);

cleanup_subscriber:
    if (subscriber) int2dds_delete_subscriber(subscriber);

cleanup_participant:
    if (participant) int2dds_delete_participant(participant);

    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Subscriber terminated.\n");
    return 0;
}
