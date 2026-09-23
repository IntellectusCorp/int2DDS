pub mod c;
pub mod csharp;
pub mod java;
pub mod python;
pub mod rpc;
pub mod rust;
pub mod xml;

use crate::types::{ResolvedUnion, ResolvedUnionLabel};

/// The implicit discriminator value of a union's `default:` member (spec §7.14.2): for a
/// signed discriminator the negative value closest to zero that no case label uses,
/// otherwise the non-negative value closest to zero that no case label uses. Shared by
/// the Rust generator (variant discriminant) and the C generator (advertised label).
pub(crate) fn union_default_discriminant(u: &ResolvedUnion, signed: bool) -> i64 {
    let used: std::collections::HashSet<i64> = u
        .cases
        .iter()
        .flat_map(|c| &c.labels)
        .filter_map(|l| match l {
            ResolvedUnionLabel::Int(v) => Some(*v),
            ResolvedUnionLabel::Bool(b) => Some(*b as i64),
            ResolvedUnionLabel::Ident(_) => None,
        })
        .collect();
    let (mut candidate, step) = if signed { (-1i64, -1i64) } else { (0i64, 1i64) };
    while used.contains(&candidate) {
        candidate += step;
    }
    candidate
}
