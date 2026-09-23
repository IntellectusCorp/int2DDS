package com.intellectus.int2dds.async;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.conditions.Condition;
import com.intellectus.int2dds.conditions.GuardCondition;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;

class AsyncWaitSetTest {
    @Test
    void waitAsyncCompletesWhenTriggeredFromAnotherThread() throws Exception {
        try (AsyncWaitSet ws = new AsyncWaitSet(); GuardCondition gc = new GuardCondition()) {
            ws.attach(gc);
            CompletableFuture<List<Condition>> f = ws.waitAsync(3000);
            assertFalse(f.isDone(), "wait should still be blocking on the executor thread");
            gc.setTriggerValue(true);
            List<Condition> hit = f.get(3, TimeUnit.SECONDS);
            assertEquals(1, hit.size());
            assertTrue(hit.get(0) == gc);
        }
    }

    @Test
    void waitAsyncTimesOutWhenNothingTriggers() throws Exception {
        try (AsyncWaitSet ws = new AsyncWaitSet(); GuardCondition gc = new GuardCondition()) {
            ws.attach(gc); // trigger stays false
            long t0 = System.nanoTime();
            List<Condition> hit = ws.waitAsync(200).get(2, TimeUnit.SECONDS);
            long ms = (System.nanoTime() - t0) / 1_000_000L;
            assertTrue(hit.isEmpty(), "no trigger -> empty");
            assertTrue(ms >= 150, "should have blocked ~200ms, was " + ms);
        }
    }
}
