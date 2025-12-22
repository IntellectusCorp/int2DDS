/**
 * int2dds FFI Listener Example - Publisher
 *
 * This example demonstrates how to use listener callbacks for DataWriter.
 * It shows the on_publication_matched callback that fires when subscribers
 * are discovered or lost.
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


/**
 * Callback: Invoked when subscribers are discovered or lost
 * This is called from a DDS background thread - must be thread-safe!
 */
void on_publication_matched(Int2DdsDataWriter* writer,
                           const Int2DdsPublicationMatchedStatus* status,
                           void* user_ctx) {
    (void)writer;
    (void)user_ctx;

    printf("\n[Callback] Publication Matched Event:\n");
    printf("  Current subscribers: %d\n", status->current_count);
    printf("  Total matched: %d\n", status->total_count);
    printf("  Change: %d (%s)\n",
           status->current_count_change,
           status->current_count_change > 0 ? "subscriber joined" : "subscriber left");
    printf("  Last subscription handle: ");
    for (int i = 0; i < 16; i++) {
        printf("%02x", status->last_subscription_handle[i]);
    }
    printf("\n\n");
}

int main(int argc, char* argv[]) {
    Int2DdsRet ret;
    Int2DdsParticipantFactory* factory = NULL;
    Int2DdsParticipant* participant = NULL;
    Int2DdsPublisher* publisher = NULL;
    Int2DdsTopic* topic = NULL;
    Int2DdsDataWriter* writer = NULL;
    Int2DdsDataWriterQos* qos = NULL;
    Int2DdsTypeDescriptor* type_desc = NULL;
    Int2DdsData* data = NULL;

    int32_t domain_id = 0;
    int use_reliable = 1;  /* Use reliable for better demonstration */

    /* Parse arguments */
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--best-effort") == 0) {
            use_reliable = 0;
        } else if (strcmp(argv[i], "--domain") == 0 && i + 1 < argc) {
            domain_id = atoi(argv[++i]);
        }
    }

    printf("=== int2dds FFI Listener Publisher ===\n");
    printf("Domain: %d, QoS: %s\n", domain_id, use_reliable ? "RELIABLE" : "BEST_EFFORT");
    printf("Topic: HelloWorldTopic\n");
    printf("======================================\n\n");
    printf("This example demonstrates DataWriter listener callbacks.\n");
    printf("Start the subscriber to see the publication_matched callback fire!\n\n");

    /* Initialize factory */
    ret = int2dds_domain_participant_factory_get_instance(&factory);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to get participant factory: %d\n", ret);
        return 1;
    }

    /* Create participant */
    ret = int2dds_create_participant(factory, NULL, domain_id, &participant);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create participant: %d\n", ret);
        return 1;
    }

    /* Create publisher */
    ret = int2dds_create_publisher(participant, &publisher);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create publisher: %d\n", ret);
        goto cleanup_participant;
    }

    /* Create type descriptor for HelloWorld */
    ret = int2dds_type_descriptor_create("HelloWorld", &type_desc);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create type descriptor: %d\n", ret);
        goto cleanup_publisher;
    }

    /* Add fields to type descriptor */
    ret = int2dds_type_descriptor_add_u32(type_desc, "index", false);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to add index field: %d\n", ret);
        goto cleanup_type_desc;
    }

    ret = int2dds_type_descriptor_add_string(type_desc, "message", 256, false);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to add message field: %d\n", ret);
        goto cleanup_type_desc;
    }

    /* Create topic with type descriptor */
    ret = int2dds_create_topic(participant, "HelloWorldTopic", type_desc, NULL, &topic);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create topic: %d\n", ret);
        goto cleanup_type_desc;
    }

    /* Configure QoS */
    ret = int2dds_datawriter_qos_create_default(&qos);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create QoS: %d\n", ret);
        goto cleanup_topic;
    }

    if (use_reliable) {
        ret = int2dds_datawriter_qos_set_reliability(qos, 1, 0);  /* RELIABLE */
    } else {
        ret = int2dds_datawriter_qos_set_reliability(qos, 0, 0);  /* BEST_EFFORT */
    }
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to set reliability QoS: %d\n", ret);
        goto cleanup_qos;
    }

    /* Configure listener with on_publication_matched callback */
    Int2DdsDataWriterListener listener = {0};
    listener.on_publication_matched = on_publication_matched;
    listener.on_offered_deadline_missed = NULL;  /* Not using other callbacks */
    listener.on_offered_incompatible_qos = NULL;
    listener.on_liveliness_lost = NULL;
    listener.user_context = NULL;  /* No user context needed for this example */

    printf("Creating DataWriter with listener...\n");

    /* Create writer with listener - ALL status changes enabled (0xFFFFFFFF) */
    ret = int2dds_create_datawriter_with_listener(
        publisher,
        topic,
        qos,
        &listener,
        0xFFFFFFFF,  /* Enable all status notifications */
        &writer
    );
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create datawriter with listener: %d\n", ret);
        goto cleanup_qos;
    }

    printf("DataWriter created successfully!\n");
    printf("Waiting for subscribers to connect...\n\n");

    /* Give some time for discovery */
    sleep_ms(2000);

    /* Create data container */
    ret = int2dds_data_create(type_desc, &data);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "Failed to create data: %d\n", ret);
        goto cleanup;
    }

    /* Publish 10 messages */
    printf("Publishing 10 messages...\n");
    char message[256];

    for (uint32_t i = 1; i <= 10; i++) {
        snprintf(message, sizeof(message), "Hello World from Publisher! Message #%u", i);

        /* Set field values */
        ret = int2dds_data_set_u32(data, "index", i);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to set index: %d\n", ret);
            continue;
        }

        ret = int2dds_data_set_string(data, "message", message);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Failed to set message: %d\n", ret);
            continue;
        }

        /* Write data (automatic CDR serialization) */
        ret = int2dds_write(writer, data);
        if (ret != INT2DDS_RET_OK) {
            fprintf(stderr, "Write failed: %d\n", ret);
        } else {
            printf("[%u] Published message: '%s'\n", i, message);
        }

        sleep_ms(1000);  /* 1 second between messages */
    }

    printf("\nAll messages published.\n");
    printf("Keeping writer alive for 5 more seconds to demonstrate subscriber disconnect...\n");
    sleep_ms(5000);

cleanup:
    /* Cleanup */
    if (data) int2dds_data_delete(data);
    if (writer) {
        printf("\nCleaning up...\n");
        int2dds_delete_datawriter(writer);
    }

cleanup_qos:
    if (qos) int2dds_datawriter_qos_destroy(qos);

cleanup_topic:
    if (topic) int2dds_delete_topic(topic);

cleanup_type_desc:
    if (type_desc) int2dds_type_descriptor_delete(type_desc);

cleanup_publisher:
    if (publisher) int2dds_delete_publisher(publisher);

cleanup_participant:
    if (participant) int2dds_delete_participant(participant);

    if (factory) int2dds_domain_participant_factory_finalize(factory);

    printf("Publisher terminated.\n");
    return 0;
}
