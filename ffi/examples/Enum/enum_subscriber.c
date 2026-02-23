/**
 * int2dds FFI Enum Type Subscriber
 *
 * Topic: EnumTestTopic
 * Type:  EnumType
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
#include "Enum.h"

static volatile sig_atomic_t g_running = 1;

static void signal_handler(int sig) {
    (void)sig;
    g_running = 0;
}

static const char* color_to_string(Color c) {
    switch (c) {
        case COLOR_RED:    return "RED";
        case COLOR_GREEN:  return "GREEN";
        case COLOR_BLUE:   return "BLUE";
        case COLOR_YELLOW: return "YELLOW";
        case COLOR_PURPLE: return "PURPLE";
        default:           return "UNKNOWN";
    }
}

static const char* status_to_string(StatusKind s) {
    switch (s) {
        case STATUS_KIND_UNKNOWN:  return "UNKNOWN";
        case STATUS_KIND_ACTIVE:   return "ACTIVE";
        case STATUS_KIND_INACTIVE: return "INACTIVE";
        case STATUS_KIND_PENDING:  return "PENDING";
        default:                   return "UNKNOWN";
    }
}

typedef struct {
    Int2DdsDataReader *reader;
    int32_t received_count;
    int32_t max_count;
} AppContext;

static void on_data_available(Int2DdsDataReader *reader, void *user_ctx) {
    AppContext *ctx = (AppContext *)user_ctx;
    uint8_t recv_buf[256];
    uintptr_t actual_size;
    bool valid_data;
    EnumType data;
    Int2DdsRet ret;

    while ((ret = int2dds_take_serialized(reader, recv_buf, sizeof(recv_buf),
                                          &actual_size, &valid_data)) == INT2DDS_RET_OK) {
        if (!valid_data) continue;

        if (EnumType_deserialize_cdr(recv_buf, actual_size, &data)) {
            printf("Received EnumType (color=%s, status=%s)\n",
                   color_to_string(data.color), status_to_string(data.status));
            ctx->received_count++;

            if (ctx->max_count > 0 && ctx->received_count >= ctx->max_count) {
                printf("\nReached target count (%d). Stopping.\n", ctx->max_count);
                g_running = 0;
                return;
            }
        } else {
            fprintf(stderr, "Deserialization failed (size=%zu)\n", (size_t)actual_size);
        }
    }
}

static void on_subscription_matched(
    Int2DdsDataReader *reader,
    const Int2DdsSubscriptionMatchedStatus *status,
    void *user_ctx)
{
    (void)reader;
    (void)user_ctx;
    if (status->current_count_change > 0) {
        printf("[Matched] Publisher connected (current: %d, total: %d)\n",
               status->current_count, status->total_count);
    } else if (status->current_count_change < 0) {
        printf("[Matched] Publisher disconnected (current: %d, total: %d)\n",
               status->current_count, status->total_count);
    }
}

static void print_usage(const char *prog) {
    printf("Usage: %s [options]\n", prog);
    printf("Options:\n");
    printf("  --domain-id <id>   Domain ID (default: 0)\n");
    printf("  --count <n>        Number of samples to receive, 0=infinite (default: 0)\n");
    printf("  --reliability <r>  best_effort or reliable (default: best_effort)\n");
    printf("  --encoding <e>     xcdr1 or xcdr2 (default: xcdr1)\n");
    printf("  --help             Show this help\n");
}

int main(int argc, char *argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsSubscriber *subscriber = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsDataReader *reader = NULL;
    Int2DdsDataReaderQos *qos = NULL;

    int32_t domain_id = 0;
    int32_t count = 0;
    int use_reliable = 0;
    int use_xcdr2 = 0;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--domain-id") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--count") == 0 && i + 1 < argc) {
            count = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--reliability") == 0 && i + 1 < argc) {
            i++;
            if (strcmp(argv[i], "reliable") == 0) use_reliable = 1;
        } else if (strcmp(argv[i], "--encoding") == 0 && i + 1 < argc) {
            i++;
            if (strcmp(argv[i], "xcdr2") == 0) use_xcdr2 = 1;
        } else if (strcmp(argv[i], "--help") == 0) {
            print_usage(argv[0]);
            return 0;
        }
    }

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("=== int2dds Enum Subscriber ===\n");
    printf("Topic: EnumTestTopic\n");
    printf("Type:  EnumType (APPENDABLE)\n");
    printf("Domain: %d, QoS: %s\n", domain_id,
           use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("Encoding: %s\n", use_xcdr2 ? "XCDR2" : "XCDR1");
    printf("Count: %s\n", count > 0 ? "" : "infinite");
    if (count > 0) printf("  %d samples\n", count);
    printf("===============================\n\n");

    AppContext app_ctx = {0};
    app_ctx.max_count = count;

    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to get participant factory: %d\n", ret); return 1; }

    ret = int2dds_create_participant(factory, "enum_subscriber", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create participant: %d\n", ret); goto cleanup; }

    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create subscriber: %d\n", ret); goto cleanup; }

    {
        Int2DdsTypeInfo *type_info = EnumType_type_info();
        ret = int2dds_create_topic_with_type_info(participant, "EnumTestTopic",
                                                   type_info, NULL, &topic);
        int2dds_type_info_destroy(type_info);
    }
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create topic: %d\n", ret); goto cleanup; }

    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create QoS: %d\n", ret); goto cleanup; }

    if (use_reliable) {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE);
    } else {
        ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_BEST_EFFORT);
    }
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to set reliability: %d\n", ret); goto cleanup; }

    if (use_xcdr2) {
        ret = int2dds_datareader_qos_set_data_representation(qos, INT2DDS_QOS_DATA_REPR_XCDR2);
        if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to set data representation: %d\n", ret); goto cleanup; }
    }

    Int2DdsDataReaderListener listener = {0};
    listener.on_data_available = on_data_available;
    listener.on_subscription_matched = on_subscription_matched;
    listener.user_context = &app_ctx;

    ret = int2dds_create_datareader_with_listener(
        subscriber, topic, qos, &listener,
        INT2DDS_STATUS_DATA_AVAILABLE | INT2DDS_STATUS_SUBSCRIPTION_MATCHED, &reader);
    if (ret != INT2DDS_RET_OK) { fprintf(stderr, "Failed to create datareader: %d\n", ret); goto cleanup; }
    app_ctx.reader = reader;

    printf("Subscriber ready. Waiting for publisher...\n");
    printf("Press Ctrl+C to stop.\n\n");

    while (g_running) {
        sleep_ms(100);
    }

cleanup:
    printf("\nShutting down...\n");
    if (reader) int2dds_delete_datareader(reader);
    if (qos) int2dds_datareader_qos_destroy(qos);
    if (topic) int2dds_delete_topic(topic);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Subscriber finished. Received %d samples.\n", app_ctx.received_count);
    return 0;
}
