/* glibc floor smoke test for libint2dds_ffi.so.
 *
 * Built inside the release builder image (glibc 2.28) and shipped as a CI
 * artifact, so the target containers need no compiler and no python — several
 * of them (ubi8, ubuntu 20.04/22.04/24.04) ship neither.
 *
 * RTLD_NOW forces full relocation at load time, so every GLIBC_* symbol
 * version the library requires must resolve here. Lazy binding would let an
 * unresolved symbol hide until first call.
 */
#include <dlfcn.h>
#include <stdio.h>

typedef int (*get_ttl_fn)(unsigned char *, _Bool *);

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s <path-to-libint2dds_ffi.so>\n", argv[0]);
        return 2;
    }

    void *h = dlopen(argv[1], RTLD_NOW);
    if (!h) {
        fprintf(stderr, "FAIL dlopen: %s\n", dlerror());
        return 1;
    }

    get_ttl_fn f = (get_ttl_fn)dlsym(h, "int2dds_env_get_multicast_ttl");
    if (!f) {
        fprintf(stderr, "FAIL dlsym: %s\n", dlerror());
        dlclose(h);
        return 1;
    }

    unsigned char ttl = 0;
    _Bool has = 0;
    int rc = f(&ttl, &has);
    printf("OK  dlopen+call rc=%d has_value=%d\n", rc, (int)has);

    dlclose(h);
    return 0;
}
