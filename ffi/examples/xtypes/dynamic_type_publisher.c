/**
 * XTypes Dynamic Type Publisher Example (C FFI)
 *
 * Publishes a SensorData topic using the type_info builder API so that the
 * TypeObject is sent during discovery. The companion dynamic_type_subscriber
 * receives this data WITHOUT knowing the type at compile time, by discovering
 * the TypeObject at runtime and decoding samples dynamically.
 *
 * SensorData layout (Final extensibility):
 *   sensor_id : int32  [KEY]
 *   temperature : float64
 *   humidity    : float64
 *   location    : string (bounded 32)
 *
 * Usage:
 *   dynamic_type_publisher [--domain <id>]
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

#include "int2dds-ffi.h"

/* ------------------------------------------------------------------ */
/* Manual CDR serialization helpers (Final extensibility, little-endian) */
/* ------------------------------------------------------------------ */

static size_t cdr_write_header(uint8_t *buf) {
    /* CDR encapsulation header: LE, options = 0 */
    buf[0] = 0x00;  /* representation id high */
    buf[1] = 0x01;  /* representation id low  (CDR_LE) */
    buf[2] = 0x00;  /* options high */
    buf[3] = 0x00;  /* options low  */
    return 4;
}

static size_t cdr_write_i32(uint8_t *buf, size_t off, int32_t v) {
    /* align to 4 */
    while (off % 4 != 0) buf[off++] = 0;
    memcpy(buf + off, &v, 4);
    return off + 4;
}

static size_t cdr_write_f64(uint8_t *buf, size_t off, double v) {
    /* align to 8 */
    while (off % 8 != 0) buf[off++] = 0;
    memcpy(buf + off, &v, 8);
    return off + 8;
}

static size_t cdr_write_string(uint8_t *buf, size_t off, const char *s) {
    uint32_t len = (uint32_t)strlen(s) + 1; /* includes NUL */
    /* align to 4 for the length prefix */
    while (off % 4 != 0) buf[off++] = 0;
    memcpy(buf + off, &len, 4);
    off += 4;
    memcpy(buf + off, s, len);
    off += len;
    return off;
}

/* Serialize SensorData to CDR bytes. Returns total length. */
static size_t serialize_sensor_data(uint8_t *buf, size_t cap,
                                    int32_t sensor_id,
                                    double temperature,
                                    double humidity,
                                    const char *location) {
    if (cap < 128) return 0; /* safety check */
    size_t off = cdr_write_header(buf);
    off = cdr_write_i32(buf, off, sensor_id);
    off = cdr_write_f64(buf, off, temperature);
    off = cdr_write_f64(buf, off, humidity);
    off = cdr_write_string(buf, off, location);
    return off;
}

/* Serialize just the key (sensor_id) for instance management. */
static size_t serialize_sensor_key(uint8_t *buf, size_t cap, int32_t sensor_id) {
    if (cap < 8) return 0;
    size_t off = cdr_write_header(buf);
    off = cdr_write_i32(buf, off, sensor_id);
    return off;
}

int main(int argc, char *argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsPublisher *publisher = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsDataWriter *writer = NULL;
    Int2DdsDataWriterQos *qos = NULL;
    Int2DdsWaitSet *waitset = NULL;
    Int2DdsTypeInfo *type_info = NULL;

    int32_t domain_id = 0;

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        }
    }

    printf("XTypes Dynamic Type Publisher (C FFI)\n");
    printf("Domain: %d  |  Final SensorData with bounded string\n", domain_id);
    printf("The XTypes Subscriber receives this using dynamic type discovery.\n\n");

    /* ---- Create DomainParticipant ---- */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    ret = int2dds_create_participant(factory, "dynamic_type_publisher", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        goto cleanup;
    }

    /* ---- Build TypeInfo for SensorData (Final extensibility) ---- */
    ret = int2dds_type_info_create("SensorData", 0 /* Final */, &type_info);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create type info: %d\n", ret);
        goto cleanup;
    }

    /* sensor_id : int32 [KEY] */
    ret = int2dds_type_info_add_field(type_info, "sensor_id",
                                      INT2DDS_FIELD_INT32, INT2DDS_MEMBER_KEY);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to add sensor_id field: %d\n", ret);
        goto cleanup;
    }

    /* temperature : float64 */
    ret = int2dds_type_info_add_field(type_info, "temperature",
                                      INT2DDS_FIELD_FLOAT64, 0);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to add temperature field: %d\n", ret);
        goto cleanup;
    }

    /* humidity : float64 */
    ret = int2dds_type_info_add_field(type_info, "humidity",
                                      INT2DDS_FIELD_FLOAT64, 0);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to add humidity field: %d\n", ret);
        goto cleanup;
    }

    /* location : string (bounded 32) */
    ret = int2dds_type_info_add_field(type_info, "location",
                                      INT2DDS_FIELD_STRING, 0);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to add location field: %d\n", ret);
        goto cleanup;
    }

    /* ---- Create Topic with TypeInfo (sends TypeObject in discovery) ---- */
    ret = int2dds_create_topic_with_type_info(participant, "SensorTopic",
                                              type_info, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup;
    }

    /* ---- Create Publisher ---- */
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    /* ---- Create DataWriter (Reliable) ---- */
    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datawriter_qos_set_reliability(qos,
            INT2DDS_QOS_RELIABILITY_RELIABLE, 1000000000 /* 1s */);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datawriter(publisher, topic, qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    /* ---- Wait for subscriber to match ---- */
    printf("Waiting for subscriber to match...\n");

    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datawriter(waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach writer: %d\n", ret);
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
    }
    printf("Starting to publish...\n\n");

    /* ---- Publish loop ---- */
    {
        const char *locations[] = {"Lab A", "Lab B", "Warehouse", "Office"};
        const int num_locations = 4;
        int32_t sensor_id = 1;
        uint32_t index = 0;
        uint8_t buf[4096];
        uint8_t key_buf[64];

        while (1) {
            double temperature = 20.0 + ((double)(index) * 0.5);
            while (temperature >= 35.0) temperature -= 15.0;

            double humidity = 40.0 + ((double)(index) * 0.3);
            while (humidity >= 70.0) humidity -= 30.0;

            const char *location = locations[index % num_locations];

            size_t data_len = serialize_sensor_data(buf, sizeof(buf),
                                                    sensor_id, temperature,
                                                    humidity, location);
            size_t key_len = serialize_sensor_key(key_buf, sizeof(key_buf),
                                                  sensor_id);

            ret = int2dds_write_serialized(writer, buf, data_len,
                                           key_buf, key_len);
            if (ret != INT2DDS_RET_OK) {
                fprintf(stderr, "Write failed: %d\n", ret);
            } else {
                printf("[SEND] id=%d temp=%.1f°C hum=%.1f%% loc=%s\n",
                       sensor_id, temperature, humidity, location);
            }

            sleep_ms(1000);
            index++;
            if (index % 5 == 0) {
                sensor_id = (sensor_id % 3) + 1;
            }
        }
    }

cleanup:
    if (waitset) {
        int2dds_waitset_detach_datawriter(waitset, writer);
        int2dds_waitset_delete(waitset);
    }
    if (writer) int2dds_delete_datawriter(writer);
    if (qos) int2dds_datawriter_qos_destroy(qos);
    if (topic) int2dds_delete_topic(topic);
    if (type_info) int2dds_type_info_destroy(type_info);
    if (publisher) int2dds_delete_publisher(publisher);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("\nPublisher finished.\n");
    return (ret == INT2DDS_RET_OK) ? 0 : 1;
}
