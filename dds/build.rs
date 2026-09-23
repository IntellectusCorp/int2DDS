fn main() {
    // No build-time linking required. Enterprise integrations hook in via the
    // static `enterprise-hooks` feature seams (see dds/src/common/enterprise_hooks.rs),
    // not dynamic library loading.
}
