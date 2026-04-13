/**
 * Keyed Data Example Publisher
 *
 * Demonstrates using keyed data types with instance management:
 * - register_instance
 * - write with key
 * - unregister_instance
 * - dispose
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

#define INT2DDS_CDR_STATIC
#include "int2dds-ffi.h"
#include "keyed_data.h"


int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsWaitSet* waitset = NULL;

    int domain_id = 0;

    printf("=== int2dds Keyed Data Publisher Example ===\n\n");

    // Initialize
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to get participant factory: %d\n", ret);
        return 1;
    }

    // Create Participant (DomainParticipant)
    ret = int2dds_create_participant(factory, "keyed_publisher", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create participant: %d\n", ret);
        int2dds_domain_participant_factory_finalize(factory);
        return 1;
    }
    printf("Created participant on domain %d\n", domain_id);

    // Create Publisher
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create publisher: %d\n", ret);
        goto cleanup;
    }

    // Create Topic with key support (Appendable extensibility)
    ret = int2dds_create_topic_keyed(participant, "KeyedDataTopic", "KeyedData",
                                     1,    /* APPENDABLE */
                                     true, /* has_key */
                                     NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create topic: %d\n", ret);
        goto cleanup;
    }
    printf("Created topic: KeyedDataTopic\n");

    // Create DataWriter with default QoS
    ret = int2dds_create_datawriter(publisher, topic, NULL, &writer);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create data writer: %d\n", ret);
        goto cleanup;
    }
    printf("Created data writer\n\n");

    // Create WaitSet and attach writer
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    ret = int2dds_waitset_attach_datawriter(waitset, writer);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to attach writer to waitset: %d\n", ret);
        goto cleanup;
    }

    // Wait for subscriber to match using WaitSet
    printf("Waiting for subscriber...\n");
    ret = int2dds_waitset_wait(waitset, -1);  // -1 for infinite wait
    if (ret != INT2DDS_RET_OK) {
        printf("WaitSet wait failed: %d\n", ret);
        goto cleanup;
    }

    // Get matched status to confirm
    {
        int32_t total_count = 0;
        int32_t current_count = 0;
        ret = int2dds_get_publication_matched_status(writer, &total_count, &current_count);
        if (ret != INT2DDS_RET_OK) {
            printf("Failed to get publication matched status: %d\n", ret);
            goto cleanup;
        }
        printf("Subscriber matched! (total: %d, current: %d)\n", total_count, current_count);
    }
    printf("Starting keyed data operations...\n");

    // Define instance keys
    {
        uint32_t keys[] = {1, 2, 3};
        int num_instances = sizeof(keys) / sizeof(keys[0]);

        // Instance handles storage
        uint8_t handles[3][16];

        // Buffers for serialization
        KeyedData kd;
        uint8_t buf[4096];
        uint8_t key_buf[64];

        // Register instances
        printf("\n--- Registering instances ---\n");
        for (int i = 0; i < num_instances; i++) {
            kd.key = keys[i];
            kd.value[0] = '\0';

            size_t key_len = KeyedData_serialize_key(&kd, key_buf, sizeof(key_buf));
            if (key_len == 0) {
                printf("Key serialization failed for key=%u\n", keys[i]);
                continue;
            }

            ret = int2dds_datawriter_register_instance(writer, key_buf, key_len,
                                                       (uint8_t(*)[16])&handles[i]);
            if (ret != INT2DDS_RET_OK) {
                printf("Failed to register instance %u: %d\n", keys[i], ret);
            } else {
                printf("Registered instance with key=%u\n", keys[i]);
            }
        }

        // Write data for each instance
        printf("\n--- Writing data ---\n");
        char message[64];

        for (int round = 0; round < 3; round++) {
            for (int i = 0; i < num_instances; i++) {
                snprintf(message, sizeof(message), "Message %d from instance %u", round + 1, keys[i]);

                kd.key = keys[i];
                snprintf(kd.value, sizeof(kd.value), "%s", message);

                size_t data_len = KeyedData_serialize_cdr(&kd, buf, sizeof(buf));
                size_t key_len = KeyedData_serialize_key(&kd, key_buf, sizeof(key_buf));
                if (data_len == 0 || key_len == 0) {
                    printf("Serialization failed\n");
                    continue;
                }

                ret = int2dds_write_serialized(writer, buf, data_len, key_buf, key_len);
                if (ret != INT2DDS_RET_OK) {
                    printf("Failed to write: %d\n", ret);
                } else {
                    printf("Written: key=%u, value=\"%s\"\n", keys[i], message);
                }
            }
            sleep_ms(500);
        }

        // Unregister instance 2 (keep data available but indicate no more updates)
        printf("\n--- Unregistering instance 2 ---\n");
        {
            kd.key = 2;
            size_t key_len = KeyedData_serialize_key(&kd, key_buf, sizeof(key_buf));
            if (key_len > 0) {
                ret = int2dds_datawriter_unregister_instance(writer, key_buf, key_len,
                                                             (uint8_t(*)[16])&handles[1]);
                if (ret != INT2DDS_RET_OK) {
                    printf("Failed to unregister instance 2: %d\n", ret);
                } else {
                    printf("Unregistered instance with key=2\n");
                }
            }
        }

        sleep_ms(500);

        // Dispose instance 3 (indicate instance is no longer valid)
        printf("\n--- Disposing instance 3 ---\n");
        {
            kd.key = 3;
            size_t key_len = KeyedData_serialize_key(&kd, key_buf, sizeof(key_buf));
            if (key_len > 0) {
                ret = int2dds_datawriter_dispose(writer, key_buf, key_len,
                                                 (uint8_t(*)[16])&handles[2]);
                if (ret != INT2DDS_RET_OK) {
                    printf("Failed to dispose instance 3: %d\n", ret);
                } else {
                    printf("Disposed instance with key=3\n");
                }
            }
        }

        // Write more data for instance 1 (still active)
        printf("\n--- Writing more data for instance 1 ---\n");
        for (int round = 0; round < 2; round++) {
            snprintf(message, sizeof(message), "Final message %d from instance 1", round + 1);

            kd.key = 1;
            snprintf(kd.value, sizeof(kd.value), "%s", message);

            size_t data_len = KeyedData_serialize_cdr(&kd, buf, sizeof(buf));
            size_t key_len = KeyedData_serialize_key(&kd, key_buf, sizeof(key_buf));
            if (data_len == 0 || key_len == 0) {
                printf("Serialization failed\n");
                continue;
            }

            ret = int2dds_write_serialized(writer, buf, data_len, key_buf, key_len);
            if (ret == INT2DDS_RET_OK) {
                printf("Written: key=1, value=\"%s\"\n", message);
            }
            sleep_ms(500);
        }
    }

    printf("\nPublishing complete\n");
    sleep_ms(1000);

cleanup:
    // Cleanup
    if (waitset) {
        int2dds_waitset_detach_datawriter(waitset, writer);
        int2dds_waitset_delete(waitset);
    }
    if (writer) int2dds_delete_datawriter(writer);
    if (topic) int2dds_delete_topic(topic);
    if (publisher) int2dds_delete_publisher(publisher);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Cleanup complete\n");
    return 0;
}
