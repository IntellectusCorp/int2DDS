#[cfg(test)]
mod tests {
    use int2dds_ffi::types::Int2DdsSampleInfo;
    use std::mem::{offset_of, size_of};

    // Java SampleInfo.decode()가 하드코딩한 오프셋을 고정한다.
    #[test]
    fn sample_info_layout_is_pinned() {
        assert_eq!(size_of::<Int2DdsSampleInfo>(), 76, "struct size changed");
        assert_eq!(offset_of!(Int2DdsSampleInfo, source_timestamp_sec), 0);
        assert_eq!(offset_of!(Int2DdsSampleInfo, source_timestamp_nanosec), 4);
        assert_eq!(offset_of!(Int2DdsSampleInfo, sample_state), 8);
        assert_eq!(offset_of!(Int2DdsSampleInfo, view_state), 12);
        assert_eq!(offset_of!(Int2DdsSampleInfo, instance_state), 16);
        assert_eq!(offset_of!(Int2DdsSampleInfo, instance_handle), 20);
        assert_eq!(offset_of!(Int2DdsSampleInfo, publication_handle), 36);
        assert_eq!(offset_of!(Int2DdsSampleInfo, disposed_generation_count), 52);
        assert_eq!(offset_of!(Int2DdsSampleInfo, no_writers_generation_count), 56);
        assert_eq!(offset_of!(Int2DdsSampleInfo, sample_rank), 60);
        assert_eq!(offset_of!(Int2DdsSampleInfo, generation_rank), 64);
        assert_eq!(offset_of!(Int2DdsSampleInfo, absolute_generation_rank), 68);
        assert_eq!(offset_of!(Int2DdsSampleInfo, valid_data), 72);
    }
}
