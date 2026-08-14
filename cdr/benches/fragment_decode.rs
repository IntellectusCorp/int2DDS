//! DATA_FRAG decode cost: copying the fragments into one buffer before decoding
//! versus decoding across the fragments in place.
//!
//! Two payload shapes, because the two paths pay different costs. Materializing
//! copies the whole payload once and then every read is a slice index. Decoding
//! in place skips that copy, but `CdrInput::Chained` walks the chunk list from
//! the front on every read that reaches the bytes. A bulk octet sequence is one
//! run, so the copy dominates; a sequence of small structs reads member by
//! member, so the walk does. The chunk-count sweep on the fine-grained arm is
//! what makes the walk visible: at a fixed payload size, only the number of
//! chunks changes.

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use int2dds_cdr::cdr::{
    ExtensibilityKind, PrimitiveSerialize, SequenceSerialize, Xcdr2Deserializer, Xcdr2Serializer,
};
use std::hint::black_box;

/// Past the default fragmentation threshold by enough that a real sample of this
/// size is always fragmented.
const PAYLOAD_BYTES: usize = 1 << 20;

/// Fragment counts for the sweep. 16 chunks of a 1 MiB payload is 64 KiB each,
/// which is the shape the receive path actually sees.
const CHUNK_COUNTS: [usize; 3] = [4, 16, 64];

struct Cell {
    a: u32,
    b: u32,
    c: u32,
}

/// `sequence<octet>` — one bulk run on the read side.
fn bulk_payload() -> Vec<u8> {
    let mut s = Xcdr2Serializer::new(true, ExtensibilityKind::Final);
    s.write_encapsulation_header().unwrap();
    let data: Vec<u8> = (0..PAYLOAD_BYTES).map(|i| i as u8).collect();
    s.serialize_byte_sequence(&data).unwrap();
    s.into_buffer()
}

/// `sequence<Cell>` where Cell is three u32 — one read per member.
fn fine_payload() -> Vec<u8> {
    let mut s = Xcdr2Serializer::new(true, ExtensibilityKind::Final);
    s.write_encapsulation_header().unwrap();
    let cells: Vec<Cell> = (0..PAYLOAD_BYTES as u32 / 12)
        .map(|i| Cell { a: i, b: i.wrapping_mul(7), c: !i })
        .collect();
    s.serialize_sequence(&cells, |s, cell| {
        s.serialize_u32(cell.a)?;
        s.serialize_u32(cell.b)?;
        s.serialize_u32(cell.c)
    })
    .unwrap();
    s.into_buffer()
}

/// Split a payload into `count` chunks the way the fragment receive path holds
/// them. The encapsulation header lives in the leading chunk, as it does on the
/// wire.
fn fragment(payload: &[u8], count: usize) -> Vec<Bytes> {
    let per = payload.len().div_ceil(count);
    payload.chunks(per).map(Bytes::copy_from_slice).collect()
}

/// What `deserialize_chained` does today for anything that is not classic CDR:
/// concatenate, then decode from the contiguous copy.
fn materialize(chunks: &[Bytes]) -> Vec<u8> {
    let total: usize = chunks.iter().map(|c| c.len()).sum();
    let mut buf = Vec::with_capacity(total);
    for chunk in chunks {
        buf.extend_from_slice(chunk);
    }
    buf
}

fn decode_bulk(d: &mut Xcdr2Deserializer) -> usize {
    d.deserialize_byte_sequence().unwrap().len()
}

fn decode_fine(d: &mut Xcdr2Deserializer) -> usize {
    d.deserialize_sequence(|d| {
        Ok(Cell { a: d.deserialize_u32()?, b: d.deserialize_u32()?, c: d.deserialize_u32()? })
    })
    .unwrap()
    .len()
}

fn bench_bulk(c: &mut Criterion) {
    let payload = bulk_payload();
    let mut group = c.benchmark_group("bulk_octet_sequence");
    group.throughput(Throughput::Bytes(payload.len() as u64));

    for count in CHUNK_COUNTS {
        let chunks = fragment(&payload, count);
        group.bench_with_input(
            BenchmarkId::new("materialize_then_decode", count),
            &chunks,
            |b, chunks| {
                b.iter(|| {
                    let buf = materialize(chunks);
                    let mut d = Xcdr2Deserializer::new(&buf).unwrap();
                    black_box(decode_bulk(&mut d))
                })
            },
        );
        group.bench_with_input(BenchmarkId::new("decode_in_place", count), &chunks, |b, chunks| {
            b.iter(|| {
                let mut d = Xcdr2Deserializer::new_chained(chunks).unwrap();
                black_box(decode_bulk(&mut d))
            })
        });
    }
    group.finish();
}

fn bench_fine(c: &mut Criterion) {
    let payload = fine_payload();
    let mut group = c.benchmark_group("fine_grained_structs");
    group.throughput(Throughput::Bytes(payload.len() as u64));

    for count in CHUNK_COUNTS {
        let chunks = fragment(&payload, count);
        group.bench_with_input(
            BenchmarkId::new("materialize_then_decode", count),
            &chunks,
            |b, chunks| {
                b.iter(|| {
                    let buf = materialize(chunks);
                    let mut d = Xcdr2Deserializer::new(&buf).unwrap();
                    black_box(decode_fine(&mut d))
                })
            },
        );
        group.bench_with_input(BenchmarkId::new("decode_in_place", count), &chunks, |b, chunks| {
            b.iter(|| {
                let mut d = Xcdr2Deserializer::new_chained(chunks).unwrap();
                black_box(decode_fine(&mut d))
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_bulk, bench_fine);
criterion_main!(benches);
