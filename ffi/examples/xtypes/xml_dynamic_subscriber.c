/**
 * XML Dynamic Type Subscriber (C FFI)
 *
 * Loads a type defined in XML at runtime — exactly like the Rust
 * `XmlTypeRegistry` example — and subscribes to it without any compile-time
 * IDL. The companion `xml_dynamic_publisher` loads the same XML and publishes.
 *
 * Usage:
 *   xml_dynamic_subscriber [--domain N] [--xml PATH] [--type NAME]
 *
 * Defaults: SensorData from the repo's dds/examples/xtypes/sensor_data.xml.
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

#define CHECK(expr, msg)                                              \
    do {                                                              \
        ret = (expr);                                                 \
        if (ret != INT2DDS_RET_OK) {                                  \
            fprintf(stderr, "%s failed: %d\n", (msg), ret);           \
            goto cleanup;                                             \
        }                                                             \
    } while (0)

static const char *str_arg(int argc, char *argv[], const char *name, const char *fallback) {
    for (int i = 1; i < argc - 1; i++) {
        if (strcmp(argv[i], name) == 0) return argv[i + 1];
    }
    return fallback;
}

static int int_arg(int argc, char *argv[], const char *name, int fallback) {
    const char *s = str_arg(argc, argv, name, NULL);
    return s ? atoi(s) : fallback;
}

int main(int argc, char *argv[]) {
    Int2DdsRet ret = INT2DDS_RET_OK;
    Int2DdsParticipantFactory *factory = NULL;
    Int2DdsParticipant *participant = NULL;
    Int2DdsXmlTypeRegistry *registry = NULL;
    Int2DdsDynamicTypeSupport *support = NULL;
    Int2DdsTopic *topic = NULL;
    Int2DdsSubscriber *subscriber = NULL;
    Int2DdsDynamicDataReader *reader = NULL;
    Int2DdsDynamicData *received = NULL;

    int domain = int_arg(argc, argv, "--domain", 0);
    const char *xml_path =
        str_arg(argc, argv, "--xml", "../../dds/examples/xtypes/sensor_data.xml");
    const char *type_name = str_arg(argc, argv, "--type", "SensorData");

    printf("XML Dynamic Type Subscriber (C FFI)\n");
    printf("Domain: %d\n", domain);
    printf("XML : %s\nType: %s\n\n", xml_path, type_name);

    CHECK(int2dds_domain_participant_factory_get_instance(&factory), "get factory");
    CHECK(int2dds_create_participant(factory, "xml_dynamic_subscriber", domain, &participant),
          "create participant");

    CHECK(int2dds_xml_type_registry_from_file(xml_path, &registry), "load XML registry");
    CHECK(int2dds_xml_type_registry_get_type_support(registry, type_name, &support),
          "get type support");

    CHECK(int2dds_create_topic_dynamic(participant, "SensorTopic", support, NULL, &topic),
          "create topic");
    CHECK(int2dds_create_subscriber(participant, &subscriber), "create subscriber");
    CHECK(int2dds_create_datareader_dynamic(subscriber, topic, support, NULL, &reader),
          "create datareader");

    printf("Waiting for a publisher and samples...\n");
    fflush(stdout);

    while (1) {
        ret = int2dds_dynamic_reader_take(reader, &received, NULL);
        if (ret == INT2DDS_RET_OK) {
            int32_t sensor_id = 0;
            double temperature = 0.0, humidity = 0.0;
            int2dds_dynamic_data_get_i32(received, "sensor_id", &sensor_id);
            int2dds_dynamic_data_get_f64(received, "temperature", &temperature);
            int2dds_dynamic_data_get_f64(received, "humidity", &humidity);
            printf("[RECV] sensor_id=%d temperature=%.1f humidity=%.1f\n",
                   sensor_id, temperature, humidity);
            fflush(stdout);
            int2dds_dynamic_data_destroy(received);
            received = NULL;
        } else if (ret == INT2DDS_RET_NO_DATA) {
            sleep_ms(50);
        } else {
            fprintf(stderr, "take failed: %d\n", ret);
            goto cleanup;
        }
    }

cleanup:
    if (received) int2dds_dynamic_data_destroy(received);
    if (reader) int2dds_dynamic_reader_destroy(reader);
    if (subscriber) int2dds_delete_subscriber(subscriber);
    if (topic) int2dds_delete_topic(topic);
    if (support) int2dds_dynamic_type_support_destroy(support);
    if (registry) int2dds_xml_type_registry_destroy(registry);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    return (ret == INT2DDS_RET_OK || ret == INT2DDS_RET_NO_DATA) ? 0 : 1;
}
