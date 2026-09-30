package kr.co.intellectus.int2dds.core;

import kr.co.intellectus.int2dds.internal.ReturnCodes;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.charset.Charset;
import java.util.Objects;
import java.util.OptionalInt;

/**
 * Environment-variable configuration the core reads when the factory
 * singleton initialises and when a participant is created. Each setter
 * changes the current process environment, so call it before the first
 * {@link DomainParticipant} is created.
 */
public final class Env {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private Env() {}

    /**
     * Sets the IPv4 multicast TTL fallback ({@code INT2DDS_MULTICAST_TTL}).
     * An explicit {@code int2dds.transport.UDPv4.multicast_ttl} property on a
     * participant's QoS takes precedence.
     *
     * @throws IllegalArgumentException if {@code ttl} is outside 0..255
     */
    public static void setMulticastTtl(int ttl) {
        if (ttl < 0 || ttl > 255) {
            throw new IllegalArgumentException("ttl must be 0..255: " + ttl);
        }
        ReturnCodes.check(FfiAccess.envSetMulticastTtl(ttl));
    }

    /** The current {@code INT2DDS_MULTICAST_TTL} override; empty when unset or not a valid byte. */
    public static OptionalInt getMulticastTtl() {
        int[] ttl = new int[1];
        boolean[] hasValue = new boolean[1];
        ReturnCodes.check(FfiAccess.envGetMulticastTtl(ttl, hasValue));
        return hasValue[0] ? OptionalInt.of(ttl[0]) : OptionalInt.empty();
    }

    /**
     * Sets the QoS profile file path(s) the factory auto-loads on first use
     * ({@code DDS_QOS_PROFILE}). Several paths may be joined with {@code ,}.
     */
    public static void setQosProfile(String path) {
        Objects.requireNonNull(path, "path");
        ReturnCodes.check(FfiAccess.envSetQosProfile(path.getBytes(UTF8)));
    }

    /**
     * Selects the {@code "Library::Profile"} that default-QoS entity creation
     * draws from ({@code DDS_DEFAULT_QOS_PROFILE}).
     */
    public static void setDefaultQosProfile(String profile) {
        Objects.requireNonNull(profile, "profile");
        ReturnCodes.check(FfiAccess.envSetDefaultQosProfile(profile.getBytes(UTF8)));
    }
}
