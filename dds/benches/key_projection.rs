//! Key projection: compiled plan vs. the DynamicData path it replaces.
//!
//! Both compute the same RTPS KeyHash from the same sample bytes. The plan walks
//! the wire; the dynamic path materializes the whole sample first, so the gap
//! grows with everything in the sample that is *not* the key.

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use int2dds::dcps::topic::type_support::{SerializationFormat, TypeSupport};
use int2dds::serialize::cdr::ExtensibilityKind;
use int2dds::xtypes::{
    deserialize_dynamic_data, serialize_dynamic_data, CompleteStructMember, CompleteStructType,
    CompleteTypeObject, DynamicData, DynamicType, DynamicTypeSupport, DynamicValue, MemberFlag,
    TryConstructKind, TypeFlag, TypeIdentifier, TypeObject, TypePlans,
};

fn member_flag(is_key: bool) -> MemberFlag {
    MemberFlag::new(TryConstructKind::Discard, false, false, false, is_key, false)
}

fn type_flag() -> TypeFlag {
    TypeFlag::new(int2dds::xtypes::ExtensibilityKind::Appendable, false, false)
}

/// `@key int32 id` followed by `payload_members` non-key members the key does not
/// need but the dynamic path decodes anyway.
fn build_type(payload_members: usize) -> (Arc<DynamicType>, DynamicTypeSupport, Vec<u8>) {
    let mut desc = CompleteStructType::new(type_flag(), "Bench".into(), None);
    desc.add_member(CompleteStructMember::new(
        0,
        member_flag(true),
        TypeIdentifier::Int32,
        "id".to_string(),
    ));
    for i in 0..payload_members {
        desc.add_member(CompleteStructMember::new(
            (i + 1) as u32,
            member_flag(false),
            TypeIdentifier::String8Small { bound: 0 },
            format!("text{i}"),
        ));
        desc.add_member(CompleteStructMember::new(
            (payload_members + i + 1) as u32,
            member_flag(false),
            TypeIdentifier::Float64,
            format!("value{i}"),
        ));
    }
    let object = CompleteTypeObject::Struct(desc);

    let dynamic_type =
        Arc::new(DynamicType::from_type_object(object.clone(), TypeIdentifier::None).unwrap());
    let support = DynamicTypeSupport::from_type_object(TypeObject::Complete(object)).unwrap();

    let mut data = DynamicData::new(dynamic_type.clone());
    data.set("id", 42i32).unwrap();
    for i in 0..payload_members {
        data.set_value(&format!("text{i}"), DynamicValue::String(format!("payload-{i}"))).unwrap();
        data.set_value(&format!("value{i}"), DynamicValue::Float64(i as f64)).unwrap();
    }

    let bytes = serialize_dynamic_data(
        &data,
        &SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Appendable,
            use_delimiters: false,
        },
    )
    .unwrap();

    (dynamic_type, support, bytes.to_vec())
}

fn key_projection(c: &mut Criterion) {
    let mut group = c.benchmark_group("key_projection");

    for payload_members in [1usize, 8, 32] {
        let (dynamic_type, support, bytes) = build_type(payload_members);
        let plans = TypePlans::compile(&dynamic_type);

        // Sanity: the two paths must agree, or the comparison is meaningless.
        let planned = plans.compute_key(&bytes).expect("plan must cover this type");
        let materialized = {
            let data = deserialize_dynamic_data(&bytes, &dynamic_type).unwrap();
            support.compute_key(&data)
        };
        assert_eq!(planned, materialized);

        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(BenchmarkId::new("plan", payload_members), &bytes, |b, bytes| {
            b.iter(|| plans.compute_key(std::hint::black_box(bytes)).unwrap())
        });
        group.bench_with_input(BenchmarkId::new("dynamic", payload_members), &bytes, |b, bytes| {
            b.iter(|| {
                let data =
                    deserialize_dynamic_data(std::hint::black_box(bytes), &dynamic_type).unwrap();
                support.compute_key(&data)
            })
        });
    }

    group.finish();
}

criterion_group!(benches, key_projection);
criterion_main!(benches);
