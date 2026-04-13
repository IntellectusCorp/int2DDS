/**
 * Keyed Data Example Subscriber
 *
 * Demonstrates receiving keyed data and observing instance lifecycle.
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
#include "keyed_data.h"


int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsSubscriber* subscriber = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataReader* reader = NULL;
    Int2DdsWaitSet* waitset = NULL;

    int domain_id = 0;

    printf("=== int2dds Keyed Data Subscriber Example ===\n\n");

    // Initialize
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to get participant factory: %d\n", ret);
        return 1;
    }

    // Create Participant (DomainParticipant)
    ret = int2dds_create_participant(factory, "keyed_subscriber", domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create participant: %d\n", ret);
        int2dds_domain_participant_factory_finalize(factory);
        return 1;
    }
    printf("Created participant on domain %d\n", domain_id);

    // Create Subscriber
    ret = int2dds_create_subscriber(participant, &subscriber);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create subscriber: %d\n", ret);
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

    // Create DataReader with default QoS
    ret = int2dds_create_datareader(subscriber, topic, NULL, &reader);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create data reader: %d\n", ret);
        goto cleanup;
    }
    printf("Created data reader\n");

    // Create WaitSet
    ret = int2dds_waitset_new(&waitset);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to create waitset: %d\n", ret);
        goto cleanup;
    }

    // Attach DataReader to WaitSet
    ret = int2dds_waitset_attach_datareader(waitset, reader);
    if (ret != INT2DDS_RET_OK) {
        printf("Failed to attach datareader to waitset: %d\n", ret);
        goto cleanup;
    }
    printf("Attached datareader to waitset\n\n");

    // Wait for publisher to match
    printf("Waiting for publisher...\n");
    {
        int32_t total_count = 0;
        int32_t current_count = 0;
        while (current_count == 0) {
            ret = int2dds_get_subscription_matched_status(reader, &total_count, &current_count);
            if (ret != INT2DDS_RET_OK) {
                printf("Failed to get subscription matched status: %d\n", ret);
                goto cleanup;
            }
            if (current_count == 0) {
                sleep_ms(100);
            }
        }
    }
    printf("Publisher matched! Waiting for keyed data...\n\n");

    // Main loop
    {
        uint8_t recv_buf[4096];
        uintptr_t actual_size;
        Int2DdsSampleInfo info;
        KeyedData kd;
        int timeout_count = 0;
        int max_timeouts = 10;  // Exit after 10 consecutive timeouts

        while (timeout_count < max_timeouts) {
            // Wait with 2 second timeout
            ret = int2dds_waitset_wait(waitset, 2000);

            if (ret == INT2DDS_RET_TIMEOUT) {
                timeout_count++;
                printf("Timeout %d/%d - no data\n", timeout_count, max_timeouts);
                continue;
            } else if (ret != INT2DDS_RET_OK) {
                printf("WaitSet wait failed: %d\n", ret);
                break;
            }

            // Reset timeout counter when data arrives
            timeout_count = 0;

            // Read all available samples
            while (1) {
                ret = int2dds_take_serialized_w_info(reader, recv_buf, sizeof(recv_buf),
                                                     &actual_size, &info);

                if (ret == INT2DDS_RET_NO_DATA) {
                    break;
                } else if (ret != INT2DDS_RET_OK) {
                    printf("Failed to take data: %d\n", ret);
                    break;
                }

                if (!info.valid_data) {
                    // Instance state change (dispose/unregister notification)
                    if (info.instance_state == INT2DDS_INSTANCE_STATE_NOT_ALIVE_DISPOSED) {
                        printf("Instance disposed\n");
                    } else if (info.instance_state == INT2DDS_INSTANCE_STATE_NOT_ALIVE_NO_WRITERS) {
                        printf("Instance unregistered (no writers)\n");
                    }
                    continue;
                }

                if (KeyedData_deserialize_cdr(recv_buf, actual_size, &kd)) {
                    printf("Received: key=%u, value=\"%s\"\n", kd.key, kd.value);
                } else {
                    printf("Deserialization failed\n");
                }
            }
        }

        printf("\nExiting after %d consecutive timeouts\n", max_timeouts);
    }

cleanup:
    // Cleanup
    if (waitset) {
        int2dds_waitset_detach_datareader(waitset, reader);
        int2dds_waitset_delete(waitset);
    }
    if (reader) int2dds_delete_datareader(reader);
    if (topic) int2dds_delete_topic(topic);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Cleanup complete\n");
    return 0;
}
