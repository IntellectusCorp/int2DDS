use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use int2dds::bytes::Bytes;
use int2dds::serialize::cdr::CdrDeserializer;

const N_FIELDS: usize = 2048;

fn field_wire(n_fields: usize) -> Vec<u8> {
    let mut w = vec![0x00, 0x01, 0x00, 0x00];
    for i in 0..n_fields {
        w.extend_from_slice(&(i as u32).to_le_bytes());
    }
    w
}

fn split(wire: &[u8], fragments: usize) -> Vec<Bytes> {
    let chunk = wire.len().div_ceil(fragments);
    wire.chunks(chunk).map(Bytes::copy_from_slice).collect()
}

fn read_all(d: &mut CdrDeserializer) -> u32 {
    let mut acc = 0u32;
    for _ in 0..N_FIELDS {
        acc = acc.wrapping_add(d.deserialize_u32().unwrap());
    }
    acc
}

// Per-field reads: every deserialize_u32 runs check_available and a chunk walk,
// so this measures the chained cursor / cached total_len directly.
fn chained_primitive_reads(c: &mut Criterion) {
    let w = field_wire(N_FIELDS);
    let mut group = c.benchmark_group("cdr_chained_u32_reads");

    group.bench_function("contiguous", |b| {
        b.iter(|| {
            let mut d = CdrDeserializer::new(black_box(&w)).unwrap();
            black_box(read_all(&mut d))
        })
    });

    for fragments in [1usize, 8, 64, 1024] {
        let chunks = split(&w, fragments);
        group.bench_with_input(BenchmarkId::new("fragments", fragments), &chunks, |b, chunks| {
            b.iter(|| {
                let mut d = CdrDeserializer::new_chained(black_box(chunks)).unwrap();
                black_box(read_all(&mut d))
            })
        });
    }
    group.finish();
}

// One length-prefixed sequence: a single bulk copy that still walks the chain.
fn chained_bulk_sequence(c: &mut Criterion) {
    let n = 4096usize;
    let mut w = vec![0x00, 0x01, 0x00, 0x00];
    w.extend_from_slice(&(n as u32).to_le_bytes());
    for i in 0..n {
        w.extend_from_slice(&(i as u32).to_le_bytes());
    }

    let mut group = c.benchmark_group("cdr_chained_u32_sequence");
    for fragments in [1usize, 8, 64, 1024] {
        let chunks = split(&w, fragments);
        group.bench_with_input(BenchmarkId::new("fragments", fragments), &chunks, |b, chunks| {
            b.iter(|| {
                let mut d = CdrDeserializer::new_chained(black_box(chunks)).unwrap();
                black_box(d.deserialize_u32_sequence().unwrap())
            })
        });
    }
    group.finish();
}

criterion_group!(benches, chained_primitive_reads, chained_bulk_sequence);
criterion_main!(benches);
