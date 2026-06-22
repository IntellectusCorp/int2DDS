/**
 * XML Parse Check (C FFI)
 *
 * Loads the sample XML type files through the FFI XML registry and confirms
 * every declared type resolves to a dynamic type support, demonstrating that
 * int2DDS reads XML type definitions through the C API.
 *
 * Usage:
 *   xml_parse_check [dir_with_xml_files]
 *
 * Default dir: ../../dds/examples/xtypes
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "int2dds-ffi.h"

/* Load one file and verify each declared type resolves. Returns 0 on success. */
static int check_file(const char *dir, const char *file) {
    char path[1024];
    snprintf(path, sizeof(path), "%s/%s", dir, file);

    Int2DdsXmlTypeRegistry *registry = NULL;
    Int2DdsRet ret = int2dds_xml_type_registry_from_file(path, &registry);
    if (ret != INT2DDS_RET_OK) {
        fprintf(stderr, "[FAIL] %s: load failed (%d)\n", file, ret);
        return 1;
    }

    size_t count = 0;
    int2dds_xml_type_registry_type_count(registry, &count);
    printf("[OK]   %s: %zu type(s)\n", file, count);

    int failures = 0;
    for (size_t i = 0; i < count; i++) {
        char name[256];
        size_t len = 0;
        if (int2dds_xml_type_registry_type_name(registry, i, name, sizeof(name), &len)
            != INT2DDS_RET_OK) {
            continue;
        }

        Int2DdsDynamicTypeSupport *support = NULL;
        ret = int2dds_xml_type_registry_get_type_support(registry, name, &support);
        if (ret == INT2DDS_RET_OK) {
            printf("         - %-32s resolved\n", name);
            int2dds_dynamic_type_support_destroy(support);
        } else {
            printf("         - %-32s FAILED (%d)\n", name, ret);
            failures++;
        }
    }

    int2dds_xml_type_registry_destroy(registry);
    return failures;
}

int main(int argc, char *argv[]) {
    const char *dir = (argc > 1) ? argv[1] : "../../dds/examples/xtypes";

    const char *files[] = {
        "sensor_data.xml",
    };
    const int num_files = (int)(sizeof(files) / sizeof(files[0]));

    printf("Checking XML type files in: %s\n\n", dir);

    int failures = 0;
    for (int i = 0; i < num_files; i++) {
        failures += check_file(dir, files[i]);
        printf("\n");
    }

    if (failures == 0) {
        printf("All XML type files parsed and resolved successfully.\n");
        return 0;
    }
    fprintf(stderr, "%d type(s) failed to resolve.\n", failures);
    return 1;
}
