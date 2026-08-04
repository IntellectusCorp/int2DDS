package com.intellectus.int2dds.qos;

import java.util.Objects;

/**
 * DomainParticipant QoS.
 *
 * <p>Every field is nullable and defaults to null, meaning "leave it to the
 * core".
 */
public class ParticipantQos {

    private UserData userData;
    private Property property;

    public ParticipantQos() {}

    public UserData getUserData() {
        return userData;
    }

    public void setUserData(UserData userData) {
        this.userData = userData;
    }

    public Property getProperty() {
        return property;
    }

    public void setProperty(Property property) {
        this.property = property;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof ParticipantQos)) {
            return false;
        }
        ParticipantQos other = (ParticipantQos) o;
        return Objects.equals(userData, other.userData) && Objects.equals(property, other.property);
    }

    @Override
    public int hashCode() {
        return Objects.hash(userData, property);
    }

    @Override
    public String toString() {
        return "ParticipantQos{userData=" + userData + ", property=" + property + "}";
    }
}
