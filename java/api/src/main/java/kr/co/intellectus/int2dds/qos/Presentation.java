package kr.co.intellectus.int2dds.qos;

import java.util.Objects;

/** Presentation QoS policy. */
public class Presentation {

    private PresentationAccessScopeKind accessScope = PresentationAccessScopeKind.INSTANCE;
    private boolean coherentAccess;
    private boolean orderedAccess;

    public Presentation() {}

    public Presentation(
            PresentationAccessScopeKind accessScope,
            boolean coherentAccess,
            boolean orderedAccess) {
        this.accessScope = accessScope;
        this.coherentAccess = coherentAccess;
        this.orderedAccess = orderedAccess;
    }

    public PresentationAccessScopeKind getAccessScope() {
        return accessScope;
    }

    public void setAccessScope(PresentationAccessScopeKind accessScope) {
        this.accessScope = accessScope;
    }

    public boolean isCoherentAccess() {
        return coherentAccess;
    }

    public void setCoherentAccess(boolean coherentAccess) {
        this.coherentAccess = coherentAccess;
    }

    public boolean isOrderedAccess() {
        return orderedAccess;
    }

    public void setOrderedAccess(boolean orderedAccess) {
        this.orderedAccess = orderedAccess;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Presentation)) {
            return false;
        }
        Presentation other = (Presentation) o;
        return accessScope == other.accessScope
                && coherentAccess == other.coherentAccess
                && orderedAccess == other.orderedAccess;
    }

    @Override
    public int hashCode() {
        return Objects.hash(accessScope, coherentAccess, orderedAccess);
    }

    @Override
    public String toString() {
        return "Presentation{accessScope="
                + accessScope
                + ", coherentAccess="
                + coherentAccess
                + ", orderedAccess="
                + orderedAccess
                + "}";
    }
}
