/**
 * XML Dynamic Type Publisher (C FFI)
 *
 * Loads a type defined in XML at runtime — exactly like the Rust
 * `XmlTypeRegistry` example — and publishes it without any compile-time IDL.
 * The companion `xml_dynamic_subscriber` loads the same XML and receives it.
 *
 * Usage:
 *   xml_dynamic_publisher [--domain N] [--xml PATH] [--type NAME]
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
    Int2DdsPublisher *publisher = NULL;
    Int2DdsDynamicDataWriter *writer = NULL;
    Int2DdsDynamicData *data = NULL;

    int domain = int_arg(argc, argv, "--domain", 0);
    const char *xml_path =
        str_arg(argc, argv, "--xml", "../../dds/examples/xtypes/sensor_data.xml");
    const char *type_name = str_arg(argc, argv, "--type", "SensorData");

    printf("XML Dynamic Type Publisher (C FFI)\n");
    printf("Domain: %d\n", domain);
    printf("XML : %s\nType: %s\n\n", xml_path, type_name);

    CHECK(int2dds_domain_participant_factory_get_instance(&factory), "get factory");
    CHECK(int2dds_create_participant(factory, "xml_dynamic_publisher", domain, &participant),
          "create participant");

    CHECK(int2dds_xml_type_registry_from_file(xml_path, &registry), "load XML registry");
    CHECK(int2dds_xml_type_registry_get_type_support(registry, type_name, &support),
          "get type support");

    CHECK(int2dds_create_topic_dynamic(participant, "SensorTopic", support, NULL, &topic),
          "create topic");
    CHECK(int2dds_create_publisher(participant, &publisher), "create publisher");
    CHECK(int2dds_create_datawriter_dynamic(publisher, topic, support, NULL, &writer),
          "create datawriter");

    printf("Waiting for a subscriber to match...\n");
    fflush(stdout);
    for (int i = 0; i < 400; i++) {
        int32_t c = 0;
        int2dds_dynamic_writer_publication_matched_count(writer, &c);
        if (c > 0) break;
        sleep_ms(50);
    }

    for (int n = 1; n <= 50; n++) {
        CHECK(int2dds_dynamic_data_create(support, &data), "create dynamic data");
        int2dds_dynamic_data_set_i32(data, "sensor_id", 42);
        int2dds_dynamic_data_set_f64(data, "temperature", 23.5);
        int2dds_dynamic_data_set_f64(data, "humidity", 48.0);
        CHECK(int2dds_dynamic_writer_write(writer, data), "write");
        int2dds_dynamic_data_destroy(data);
        data = NULL;
        printf("[SEND] #%d sensor_id=42 temperature=23.5 humidity=48.0\n", n);
        fflush(stdout);
        sleep_ms(300);
    }

cleanup:
    if (data) int2dds_dynamic_data_destroy(data);
    if (writer) int2dds_dynamic_writer_destroy(writer);
    if (publisher) int2dds_delete_publisher(publisher);
    if (topic) int2dds_delete_topic(topic);
    if (support) int2dds_dynamic_type_support_destroy(support);
    if (registry) int2dds_xml_type_registry_destroy(registry);
    if (participant) int2dds_delete_participant(participant);
    if (factory) int2dds_domain_participant_factory_finalize(factory);

    return (ret == INT2DDS_RET_OK) ? 0 : 1;
}
