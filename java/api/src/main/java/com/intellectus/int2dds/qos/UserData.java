package com.intellectus.int2dds.qos;

import java.util.Arrays;

/**
 * User data QoS policy.
 *
 * <p>{@code data} is copied on the way in and out so that a later mutation of
 * the caller's array cannot change QoS that was already applied.
 */
public class UserData {

    private byte[] data = new byte[0];

    public UserData() {}

    public UserData(byte[] data) {
        this.data = Arrays.copyOf(data, data.length);
    }

    /** Returns a defensive copy; mutating it has no effect on this policy. */
    public byte[] getData() {
        return Arrays.copyOf(data, data.length);
    }

    public void setData(byte[] data) {
        this.data = Arrays.copyOf(data, data.length);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof UserData)) {
            return false;
        }
        UserData other = (UserData) o;
        return Arrays.equals(data, other.data);
    }

    @Override
    public int hashCode() {
        return Arrays.hashCode(data);
    }

    @Override
    public String toString() {
        return "UserData{data=" + Arrays.toString(data) + "}";
    }
}
