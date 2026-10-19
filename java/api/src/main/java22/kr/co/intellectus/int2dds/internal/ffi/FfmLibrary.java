package kr.co.intellectus.int2dds.internal.ffi;

import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import kr.co.intellectus.int2dds.internal.NativeLoader;

/**
 * The symbols of {@code int2dds_ffi} as {@code NativeLoader} opened and version-checked them, for
 * the FFM {@code Ffi} and {@code FfiHandwritten} to bind against.
 *
 * <p>{@code NativeLoader.symbolLookup()} is package-private, because a class under {@code
 * META-INF/versions/22} may not add public API to its base version. This package reaches it with a
 * private lookup, which needs nothing beyond both classes sharing a module.
 */
final class FfmLibrary {
    private FfmLibrary() {}

    /** Opened on first use, so {@link FfiHandwritten#backend()} never loads the library. */
    static final SymbolLookup LOOKUP = open();

    // Runs inside a static initializer, so the JVM reports whatever this throws as the cause of
    // an ExceptionInInitializerError -- a NativeLoadException included, exactly as when Ffi
    // called NativeLoader directly. FfiAccess loads the library first, through NativeLoader,
    // so a load failure normally surfaces there.
    private static SymbolLookup open() {
        try {
            MethodHandle symbolLookup =
                    MethodHandles.privateLookupIn(NativeLoader.class, MethodHandles.lookup())
                            .findStatic(
                                    NativeLoader.class,
                                    "symbolLookup",
                                    MethodType.methodType(SymbolLookup.class));
            return (SymbolLookup) symbolLookup.invokeExact();
        } catch (RuntimeException | Error e) {
            throw e;
        } catch (Throwable t) {
            // ReflectiveOperationException: NativeLoader.symbolLookup() was renamed or removed.
            throw new IllegalStateException("cannot reach NativeLoader.symbolLookup()", t);
        }
    }
}
