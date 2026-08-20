package com.intellectus.int2dds.conditions;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

class WaitSetTest {
    @Test
    void waitReturnsATriggeredGuardCondition() {
        try (WaitSet ws = new WaitSet(); GuardCondition gc = new GuardCondition()) {
            ws.attach(gc);
            gc.setTriggerValue(true);
            List<Condition> hit = ws.await(2000L); // already triggered -> returns immediately
            assertEquals(1, hit.size(), "the triggered guard condition must be returned");
            assertTrue(hit.get(0) == gc);
        }
    }

    @Test
    void waitTimesOutWhenNothingTriggers() {
        try (WaitSet ws = new WaitSet(); GuardCondition gc = new GuardCondition()) {
            ws.attach(gc); // trigger stays false
            long t0 = System.nanoTime();
            List<Condition> hit = ws.await(200);
            long ms = (System.nanoTime() - t0) / 1_000_000L;
            assertTrue(hit.isEmpty(), "no trigger -> empty");
            assertTrue(ms >= 150, "should have blocked ~200ms, was " + ms);
        }
    }
}
