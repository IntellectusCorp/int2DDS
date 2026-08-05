package com.intellectus.int2dds.types;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;

/**
 * A hand-written stand-in for what the IDL backend will generate.
 *
 * <p>Three fields covering an integer, a float and a string, which is enough to
 * exercise alignment, an 8-byte value and a length-prefixed one. The field
 * order and types must stay in step with the type object built in
 * {@code CdrConformanceTest} — the whole point of that test is that the two
 * agree.
 *
 * <p>APPENDABLE, so the struct is wrapped in a DHEADER.
 */
public final class ConformanceRecord implements IDdsType {

    public int id;
    public double value;
    public String label = "";

    @Override
    public String typeName() {
        return "ConformanceRecord";
    }

    @Override
    public Extensibility extensibility() {
        return Extensibility.APPENDABLE;
    }

    @Override
    public void serializeCdr(CdrWriter writer) {
        int token = writer.dheaderBegin();
        writer.writeI32(id);
        writer.writeF64(value);
        writer.writeString(label);
        writer.dheaderFinalize(token);
    }

    @Override
    public void deserializeCdr(CdrReader reader) {
        CdrReader.Dheader d = reader.readDheader();
        id = reader.readI32();
        value = reader.readF64();
        label = reader.readString();
        reader.readDheaderEnd(d);
    }
}
