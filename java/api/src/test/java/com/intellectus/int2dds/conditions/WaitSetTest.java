package com.intellectus.int2dds.conditions;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
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

    @Test
    void attachIsIdempotent() {
        try (WaitSet ws = new WaitSet(); GuardCondition gc = new GuardCondition()) {
            ws.attach(gc);
            ws.attach(gc); // re-attaching the same instance must be a no-op
            gc.setTriggerValue(true);
            List<Condition> hit = ws.await(500L);
            assertEquals(1, hit.size(), "duplicate attach must not duplicate a trigger");
            assertTrue(hit.get(0) == gc);
        }
    }

    @Test
    void attachRejectsClosedCondition() {
        try (WaitSet ws = new WaitSet()) {
            GuardCondition gc = new GuardCondition();
            gc.close();
            assertThrows(IllegalStateException.class, () -> ws.attach(gc));
        }
    }

    @Test
    void awaitSkipsAClosedAttachedCondition() {
        try (WaitSet ws = new WaitSet(); GuardCondition open = new GuardCondition()) {
            GuardCondition doomed = new GuardCondition();
            ws.attach(open);
            ws.attach(doomed);
            open.setTriggerValue(true);
            doomed.close(); // closed while still attached: contract violation
            List<Condition> hit = ws.await(500L);
            assertEquals(1, hit.size(), "closed-attached condition must be skipped, not thrown on");
            assertTrue(hit.get(0) == open);
        }
    }

    @Test
    void detachAfterCloseIsSafe() {
        // Normal order: detach before close -> no exception either way.
        try (WaitSet ws = new WaitSet()) {
            GuardCondition gc = new GuardCondition();
            ws.attach(gc);
            ws.detach(gc);
            gc.close();
        }

        // Contract-violating order: condition closed while still attached.
        // detach() must not forward the now-freed handle to native code; it
        // just drops the bookkeeping entry. We never await() afterwards, so
        // the dangling native attachment this leaves behind is never
        // exercised.
        try (WaitSet ws = new WaitSet()) {
            GuardCondition gc = new GuardCondition();
            ws.attach(gc);
            gc.close();
            assertDoesNotThrow(() -> ws.detach(gc));
        }
    }
}
