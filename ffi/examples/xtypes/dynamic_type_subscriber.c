/**
 * XTypes Dynamic Type Subscriber Example (C FFI)
 *
 * Receives data WITHOUT knowing the type at compile time.
 * Discovers the TypeObject from the publisher via the Discovery protocol,
 * then uses the dynamic sample getter API to inspect fields at runtime.
 *
 * How it works:
 *   1. Waits for a publisher on "SensorTopic" via builtin DCPSPublication
 *   2. Extracts TypeObject from the discovered publication
 *   3. Introspects type members (name, kind, flags)
 *   4. Creates a topic backed by the discovered TypeObject
 *   5. Creates a DataReader and decodes samples field-by-field
 *
 * Usage:
 *   dynamic_type_subscriber [--domain <id>] [--topic <name>]
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

#include "int2dds-ffi.h"

/* Map field kind constant to a human-readable name. */
static const char *field_kind_name(int32_t kind) {
    switch (kind) {
        case INT2DDS_FIELD_BOOL:    return "bool";
        case INT2DDS_FIELD_BYTE:    return "byte";
        case INT2DDS_FIELD_CHAR8:   return "char8";
        case INT2DDS_FIELD_CHAR16:  return "char16";
        case INT2DDS_FIELD_INT8:    return "int8";
        case INT2DDS_FIELD_INT16:   return "int16";
        case INT2DDS_FIELD_INT32:   return "int32";
        case INT2DDS_FIELD_INT64:   return "int64";
        case INT2DDS_FIELD_UINT8:   return "uint8";
        case INT2DDS_FIELD_UINT16:  return "uint16";
        case INT2DDS_FIELD_UINT32:  return "uint32";
        case INT2DDS_FIELD_UINT64:  return "uint64";
        case INT2DDS_FIELD_FLOAT32: return "float32";
        case INT2DDS_FIELD_FLOAT64: return "float64";
        case INT2DDS_FIELD_STRING:  return "string";
        case INT2DDS_FIELD_WSTRING: return "wstring";
        default:                    return "unknown";
    }
}

static const char *extensibility_name(int32_t ext) {
    switch (ext) {
        case 0: return "Final";
        case 1: return "Appendable";
        case 2: return "Mutable";
        default: return "Unknown";
    }
}

int main(int argc, char *argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsSubscriber *subscriber = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsDataReader *reader = NULL;
    Int2DdsDataReaderQos *qos = NULL;
    Int2DdsWaitSet *waitset = NULL;
    Int2DdsTypeObject *type_obj = NULL;

    int32_t domain_id = 0;
    const char *topic_name = "SensorTopic";

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--topic") == 0 && i + 1 < argc) {
            topic_name = argv[++i];
        }
    }

    printf("XTypes Dynamic Type Subscriber (C FFI)\n");
    printf("Domain: %d  |  Waiting for publisher on topic '%s'...\n\n", domain_id, topic_name);

    /* ---- Create DomainParticipant ---- */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    ret = int2dds_create_participant(factory, "dynamic_type_subscriber", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* ---- Discover TypeObject from publisher ---- */
    {
        char type_name_buf[256];
        size_t type_name_len = 0;

        printf("Waiting for publisher with TypeObject...\n");

        ret = int2dds_wait_for_type_object(
            participant,
            topic_name,
            -1, /* infinite timeout */
            &type_obj,
            type_name_buf, sizeof(type_name_buf),
            &type_name_len);

        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to discover TypeObject: %d\n", ret);
            goto cleanup;
        }

        printf("Discovered type: %s\n", type_name_buf);

        /* ---- Introspect discovered type ---- */
        int32_t extensibility = -1;
        ret = int2dds_type_object_extensibility(type_obj, &extensibility);
        if (ret == INT2DDS_RET_OK) {
            printf("Extensibility: %s\n", extensibility_name(extensibility));
        }

        uint32_t member_count = 0;
        ret = int2dds_type_object_member_count(type_obj, &member_count);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to get member count: %d\n", ret);
            goto cleanup;
        }

        printf("Members (%u):\n", member_count);
        for (uint32_t i = 0; i < member_count; i++) {
            Int2DdsMemberInfo info;
            char name_buf[128];
            size_t name_len = 0;

            ret = int2dds_type_object_member_info(type_obj, i, &info);
            if (ret != INT2DDS_RET_OK) continue;

            ret = int2dds_type_object_member_name(type_obj, i, name_buf, sizeof(name_buf), &name_len);
            if (ret != INT2DDS_RET_OK) continue;

            printf("  - %s: %s (id=%u)%s\n",
                   name_buf,
                   field_kind_name(info.kind),
                   info.member_id,
                   (info.flags & INT2DDS_MEMBER_KEY) ? " [KEY]" : "");
        }
        printf("\n");

        /* ---- Create Topic backed by discovered TypeObject ---- */
        ret = int2dds_create_topic_with_type_object(
            participant, topic_name, type_name_buf, type_obj, NULL, &topic);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to create topic: %d\n", ret);
            goto cleanup;
        }
    }

    /* ---- Create Subscriber ---- */
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    /* ---- Create DataReader (Reliable) ---- */
    ret = int2dds_datareader_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datareader_qos_set_reliability(qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 1000000000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datareader(subscriber, topic, qos, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    /* ---- Wait for subscription matched ---- */
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datareader(waitset, reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach reader: %d\n", ret);
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
        printf("Matched with publisher! (total: %d, current: %d)\n", total_count, current_count);
    }
    printf("Receiving data...\n\n");

    /* ---- Receive loop ---- */
    {
        uint8_t recv_buf[4096];
        uintptr_t actual_size;
        bool valid_data;

        while (1) {
            ret = int2dds_take_serialized(reader, recv_buf, sizeof(recv_buf),
                                          &actual_size, &valid_data);

            if (ret == INT2DDS_RET_OK && valid_data) {
                /* Decode fields dynamically using the discovered TypeObject */
                int32_t sensor_id = 0;
                double temperature = 0.0;
                double humidity = 0.0;
                char location[256];
                size_t loc_len = 0;

                int2dds_dynamic_sample_get_i32(
                    recv_buf, actual_size, type_obj, "sensor_id", &sensor_id);
                int2dds_dynamic_sample_get_f64(
                    recv_buf, actual_size, type_obj, "temperature", &temperature);
                int2dds_dynamic_sample_get_f64(
                    recv_buf, actual_size, type_obj, "humidity", &humidity);
                int2dds_dynamic_sample_get_string(
                    recv_buf, actual_size, type_obj, "location",
                    location, sizeof(location), &loc_len);

                printf("[RECV] id=%d temp=%.1f°C hum=%.1f%% loc=%s\n",
                       sensor_id, temperature, humidity, location);
            } else if (ret == INT2DDS_RET_NO_DATA) {
                int2dds_waitset_wait(waitset, 5000);
            }
        }
    }

cleanup:
    if (waitset) {
        int2dds_waitset_detach_datareader(waitset, reader);
        int2dds_waitset_delete(waitset);
    }
    if (reader) int2dds_delete_datareader(reader);
    if (qos) int2dds_datareader_qos_destroy(qos);
    if (topic) int2dds_delete_topic(topic);
    if (type_obj) int2dds_type_object_destroy(type_obj);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("\nSubscriber finished.\n");
    return (ret == INT2DDS_RET_OK || ret == INT2DDS_RET_NO_DATA) ? 0 : 1;
}
