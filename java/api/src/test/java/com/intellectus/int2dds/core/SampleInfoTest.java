package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

class SampleInfoTest {

    @Test
    void decodesReprCStructByOffset() {
        ByteBuffer s = ByteBuffer.allocateDirect(SampleInfo.STRUCT_SIZE).order(ByteOrder.nativeOrder());
        s.putInt(0, 111);            // source_timestamp_sec
        s.putInt(4, 222);            // source_timestamp_nanosec
        s.putInt(8, 1);              // sample_state
        s.putInt(12, 2);             // view_state
        s.putInt(16, 4);             // instance_state
        for (int i = 0; i < 16; i++) {
            s.put(20 + i, (byte) (i + 1));    // instance_handle
            s.put(36 + i, (byte) (i + 100));  // publication_handle
        }
        s.putInt(52, 5);
        s.putInt(56, 6);
        s.putInt(60, 7);
        s.putInt(64, 8);
        s.putInt(68, 9);
        s.put(72, (byte) 1);         // valid_data

        SampleInfo info = SampleInfo.decode(s);

        assertEquals(111, info.sourceTimestampSec());
        assertEquals(222, info.sourceTimestampNanosec());
        assertEquals(1, info.sampleState());
        assertEquals(2, info.viewState());
        assertEquals(4, info.instanceState());
        assertEquals(5, info.disposedGenerationCount());
        assertEquals(9, info.absoluteGenerationRank());
        assertTrue(info.validData());
        byte[] expInst = new byte[16];
        for (int i = 0; i < 16; i++) {
            expInst[i] = (byte) (i + 1);
        }
        assertArrayEquals(expInst, info.instanceHandle());
    }

    @Test
    void handleAccessorsCopyDefensively() {
        byte[] inst = new byte[16];
        byte[] pub = new byte[16];
        SampleInfo info = new SampleInfo(0, 0, 0, 0, 0, inst, pub, 0, 0, 0, 0, 0, false);
        assertNotSame(inst, info.instanceHandle());
        info.instanceHandle()[0] = 42;
        assertEquals(0, info.instanceHandle()[0]);   // 외부 변조 불가
    }
}
