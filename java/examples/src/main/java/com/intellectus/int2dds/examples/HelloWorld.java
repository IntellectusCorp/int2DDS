package com.intellectus.int2dds.examples;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.types.IDdsType;

/**
 * The sample type {@link HelloWorldPub} publishes.
 *
 * <p>Two fields — an index and a message — hand-written the same way the
 * API's own test suite stands in for what the IDL backend will eventually
 * generate from {@code idl/input/HelloWorld.idl}. Its {@link #typeName()} and
 * the topic name {@link HelloWorldPub} creates it under must match what the
 * C# and Rust {@code HelloWorldPub} examples use, so all three can
 * interoperate on the wire.
 *
 * <p>APPENDABLE. That wraps the struct in a DHEADER only under XCDR2, not
 * under the XCDR1 this type actually ships under: {@link HelloWorldPub}
 * never sets an XCDR2 {@code DataRepresentation} on its writer's QoS, so
 * {@code DataWriter.resolveXcdr2} resolves to XCDR1, where {@link
 * CdrWriter#dheaderBegin()} returns {@code -1} and writes nothing -- no
 * DHEADER goes on the wire for this type.
 */
public final class HelloWorld implements IDdsType {

    public int index;
    public String message = "";

    @Override
    public String typeName() {
        return "HelloWorld";
    }

    @Override
    public Extensibility extensibility() {
        return Extensibility.APPENDABLE;
    }

    @Override
    public void serializeCdr(CdrWriter writer) {
        int token = writer.dheaderBegin();
        writer.writeU32(index);
        writer.writeString(message);
        writer.dheaderFinalize(token);
    }

    @Override
    public void deserializeCdr(CdrReader reader) {
        CdrReader.Dheader d = reader.readDheader();
        index = reader.readU32();
        message = reader.readString();
        reader.readDheaderEnd(d);
    }
}
