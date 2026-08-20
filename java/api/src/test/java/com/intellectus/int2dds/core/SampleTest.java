package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;

import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

class SampleTest {

    @Test
    void carriesDataAndInfo() {
        SampleInfo info = new SampleInfo(0, 0, 0, 0, 0, new byte[16], new byte[16],
                0, 0, 0, 0, 0, true);
        ConformanceRecord data = new ConformanceRecord();
        Sample<ConformanceRecord> sample = new Sample<ConformanceRecord>(data, info);
        assertSame(data, sample.data());
        assertSame(info, sample.info());
    }

    @Test
    void invalidSampleHasNullData() {
        SampleInfo info = new SampleInfo(0, 0, 0, 0, 0, new byte[16], new byte[16],
                0, 0, 0, 0, 0, false);
        Sample<ConformanceRecord> sample = new Sample<ConformanceRecord>(null, info);
        assertNull(sample.data());
        assertSame(info, sample.info());
    }
}
