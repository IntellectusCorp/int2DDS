/**
 * int2dds FFI Multiple Participant Example
 *
 * This example demonstrates how to create multiple DomainParticipants
 * in the same process and communicate between them.
 *
 * - Participant 1: Publisher
 * - Participant 2: Subscriber
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

/* HelloWorld type */
typedef struct HelloWorld {
    uint32_t index;
    char message[257];
} HelloWorld;

static size_t HelloWorld_serialize_cdr(const HelloWorld *val, uint8_t *buf, size_t capacity) {
    Int2DdsCdrWriter w;
    int2dds_cdr_writer_init(&w, buf, capacity, true, true);
    int2dds_cdr_write_encapsulation(&w, INT2DDS_CDR_APPENDABLE);
    size_t dh = 0;
    int2dds_cdr_write_dheader_begin(&w, &dh);
    int2dds_cdr_write_u32(&w, val->index);
    int2dds_cdr_write_string(&w, val->message);
    int2dds_cdr_write_dheader_finalize(&w, dh);
    return w.error == INT2DDS_CDR_OK ? int2dds_cdr_writer_size(&w) : 0;
}

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


int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;

    /* Participant 1 (Publisher) */
    Int2DdsParticipant* participant1 = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic1 = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* writer_qos = NULL;

    /* Participant 2 (Subscriber) */
    Int2DdsParticipant* participant2 = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic2 = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsDataReaderQos* reader_qos = NULL;

    Int2DdsWaitSet* writer_waitset = NULL;
    Int2DdsWaitSet* reader_waitset = NULL;

    int32_t domain_id = 0;
    int num_messages = 10;

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--count") == 0 && i + 1 < argc) {
            num_messages = atoi(argv[++i]);
        }
    }

    printf("int2dds FFI Multiple Participant Example\n");
    printf("Domain: %d, Messages: %d\n", domain_id, num_messages);
    printf("-----------------------------------------\n\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* ========== Create Participant 1 (Publisher) ========== */
    printf("Creating Participant 1 (Publisher)...\n");

    ret = int2dds_create_participant(factory, "participant_1_publisher", domain_id, &participant1);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant1: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_publisher(participant1, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(participant1, "HelloWorld", "HelloWorld",
                               1,  /* APPENDABLE */
                               NULL, &topic1);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic1: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datawriter_qos_create_default(&writer_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create writer QoS: %d\n", ret);
        goto cleanup;
    }

    /* Use reliable QoS for inter-participant communication */
    ret = int2dds_datawriter_qos_set_reliability(writer_qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set writer reliability: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datawriter(publisher, topic1, writer_qos, &writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter: %d\n", ret);
        goto cleanup;
    }

    printf("  Participant 1 ready!\n");

    /* ========== Create Participant 2 (Subscriber) ========== */
    printf("Creating Participant 2 (Subscriber)...\n");

    ret = int2dds_create_participant(factory, "participant_2_subscriber", domain_id, &participant2);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant2: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_subscriber(participant2, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create subscriber: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_topic(participant2, "HelloWorld", "HelloWorld",
                               1,  /* APPENDABLE */
                               NULL, &topic2);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic2: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datareader_qos_create_default(&reader_qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create reader QoS: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_datareader_qos_set_reliability(reader_qos, INT2DDS_QOS_RELIABILITY_RELIABLE, 100000000);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reader reliability: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_create_datareader(subscriber, topic2, reader_qos, &reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datareader: %d\n", ret);
        goto cleanup;
    }

    printf("  Participant 2 ready!\n\n");

    /* Create WaitSets for both writer and reader */
    ret = int2dds_waitset_new(&writer_waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create writer waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datawriter(writer_waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach writer to waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_new(&reader_waitset);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create reader waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datareader(reader_waitset, reader);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to attach reader to waitset: %d\n", ret);
        goto cleanup;
    }

    /* Wait for discovery using WaitSet */
    printf("Waiting for discovery...\n");
    ret = int2dds_waitset_wait(writer_waitset, -1);  /* -1 for infinite wait */
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    /* Get matched status to confirm */
    {
        int32_t total_count = 0;
        int32_t current_count = 0;
        ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to get publication matched status: %d\n", ret);
            goto cleanup;
        }
        printf("Discovery complete! (total: %d, current: %d)\n", total_count, current_count);
    }

    /* ========== Communication Loop ========== */
    printf("\nStarting communication between participants...\n\n");

    {
        HelloWorld hw_send, hw_recv;
        uint8_t send_buf[4096];
        uint8_t recv_buf[4096];
        uintptr_t actual_size;
        bool valid_data;
        int received_count = 0;

        for (int i = 0; i < num_messages; i++) {
            /* Publish message from Participant 1 */
            hw_send.index = i;
            snprintf(hw_send.message, sizeof(hw_send.message), "Message from Participant 1 [%d]", i);

            size_t serialized_len = HelloWorld_serialize_cdr(&hw_send, send_buf, sizeof(send_buf));
            if (serialized_len == 0) {
                fprintf(stderr, "Serialization failed\n");
                continue;
            }

            ret = int2dds_write_serialized(writer, send_buf, serialized_len, NULL, 0);
            if (ret != INT2DDS_RET_OK) {
                fprintf(stderr, "Failed to write: %d\n", ret);
                continue;
            }
            printf("[Participant 1] Sent: %s\n", hw_send.message);

            /* Small delay for data propagation */
            sleep_ms(100);

            /* Receive message at Participant 2 */
            int attempts = 0;
            while (attempts < 10) {
                ret = int2dds_take_serialized(reader, recv_buf, sizeof(recv_buf),
                                              &actual_size, &valid_data);

                if (ret == INT2DDS_RET_OK && valid_data) {
                    if (HelloWorld_deserialize_cdr(recv_buf, actual_size, &hw_recv)) {
                        printf("[Participant 2] Received: %s\n", hw_recv.message);
                        received_count++;
                    }
                    break;
                } else if (ret == INT2DDS_RET_NO_DATA) {
                    sleep_ms(50);
                    attempts++;
                } else {
                    break;
                }
            }

            if (attempts >= 10) {
                printf("[Participant 2] Timeout waiting for message %d\n", i);
            }

            printf("\n");
            sleep_ms(500);
        }

        printf("-----------------------------------------\n");
        printf("Communication complete!\n");
        printf("Sent: %d messages, Received: %d messages\n", num_messages, received_count);
    }

cleanup:
    /* Cleanup WaitSets */
    if (reader_waitset) {
        int2dds_waitset_detach_datareader(reader_waitset, reader);
        int2dds_waitset_delete(reader_waitset);
    }
    if (writer_waitset) {
        int2dds_waitset_detach_datawriter(writer_waitset, writer);
        int2dds_waitset_delete(writer_waitset);
    }

    /* Cleanup Participant 2 */
    if (reader) int2dds_delete_datareader(reader);
    if (reader_qos) int2dds_datareader_qos_destroy(reader_qos);
    if (topic2) int2dds_delete_topic(topic2);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (participant2) int2dds_delete_participant(participant2);

    /* Cleanup Participant 1 */
    if (writer) int2dds_delete_datawriter(writer);
    if (writer_qos) int2dds_datawriter_qos_destroy(writer_qos);
    if (topic1) int2dds_delete_topic(topic1);
    if (publisher) int2dds_delete_publisher(publisher);
    if (participant1) int2dds_delete_participant(participant1);

    /* Cleanup factory */
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    return 0;
}
