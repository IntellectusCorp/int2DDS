package com.intellectus.int2dds.core;

import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.types.IDdsType;
import java.nio.charset.Charset;
import java.util.Objects;

/**
 * A SQL-92-like filtered view of a {@link Topic}: a reader created on this
 * (through {@link Subscriber#createDataReader(ContentFilteredTopic,
 * java.util.function.Supplier)}) only receives samples for which the filter
 * expression evaluates true.
 *
 * <p>Created through {@link DomainParticipant#createContentFilteredTopic},
 * which registers this instance in the participant's weak child list -- the
 * same shape as {@link Topic}, whose own doc explains why that constructor is
 * package-private rather than public.
 *
 * <p>Holds its related {@link Topic} strongly, purely to keep it reachable:
 * the native ContentFilteredTopic references the related topic by handle, so
 * nothing here may let it become phantom-reachable (and be reaped) while this
 * still exists.
 *
 * @param <T> the DDS data type carried by {@link #relatedTopic()}.
 */
public final class ContentFilteredTopic<T extends IDdsType> extends NativeEntity {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private final String name;
    private final Topic<T> relatedTopic;

    ContentFilteredTopic(DomainParticipant participant, String name, Topic<T> relatedTopic,
            String filterExpression, String[] params) {
        super(Objects.requireNonNull(participant, "participant"),
                create(participant, Objects.requireNonNull(name, "name"),
                        Objects.requireNonNull(relatedTopic, "relatedTopic"),
                        Objects.requireNonNull(filterExpression, "filterExpression"), params),
                FfiAccess::deleteContentFilteredTopic);
        this.name = name;
        this.relatedTopic = relatedTopic;
    }

    /** The name this ContentFilteredTopic was created with. */
    public String name() {
        return name;
    }

    /** The {@link Topic} this filter was built against. */
    public Topic<T> relatedTopic() {
        return relatedTopic;
    }

    /** Replaces both the filter expression and its positional parameters. */
    public void setFilterExpression(String expr, String... params) {
        Objects.requireNonNull(expr, "expr");
        byte[][] paramBytes = utf8All(params);
        int rc = FfiAccess.contentFilteredTopicSetFilterExpression(
                handle(), utf8(expr), paramBytes, paramBytes.length);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** Replaces the filter's positional parameters, keeping its expression unchanged. */
    public void setExpressionParameters(String... params) {
        byte[][] paramBytes = utf8All(params);
        int rc = FfiAccess.contentFilteredTopicSetExpressionParameters(
                handle(), paramBytes, paramBytes.length);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** Enables or disables filtering; disabled means every sample passes through unfiltered. */
    public void setEnabled(boolean enabled) {
        int rc = FfiAccess.contentFilteredTopicSetEnabled(handle(), enabled);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    private static <T extends IDdsType> long create(DomainParticipant participant, String name,
            Topic<T> relatedTopic, String filterExpression, String[] params) {
        byte[] nameBytes = utf8(name);
        byte[] filterBytes = utf8(filterExpression);
        byte[][] paramBytes = utf8All(params);
        long participantHandle = participant.handle();
        long relatedHandle = relatedTopic.handle();
        long[] handleOut = new long[1];
        int rc = FfiAccess.createContentFilteredTopic(participantHandle, nameBytes, relatedHandle,
                filterBytes, paramBytes, paramBytes.length, handleOut);
        // participantHandle/relatedHandle are bare longs read moments before the
        // native call that consumes them; keep participant and relatedTopic
        // reachable across it -- see NativeKeepAlive's own doc for the full
        // argument.
        NativeKeepAlive.keepAlive(participant);
        NativeKeepAlive.keepAlive(relatedTopic);
        ReturnCodes.check(rc);
        return handleOut[0];
    }

    private static byte[][] utf8All(String[] params) {
        if (params == null) {
            return new byte[0][];
        }
        byte[][] out = new byte[params.length][];
        for (int i = 0; i < params.length; i++) {
            out[i] = utf8(params[i]);
        }
        return out;
    }

    /** UTF-8 bytes for a name/expression/parameter that crosses as byte[]. Never a String. */
    private static byte[] utf8(String s) {
        return s.getBytes(UTF8);
    }
}
