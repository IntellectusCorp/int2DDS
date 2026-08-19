pub mod c;
pub mod csharp;
pub mod python;
pub mod rpc;
pub mod rust;
pub mod xml;

use crate::types::ResolvedType;

/// Split a possibly multidimensional array into its dimensions and base element.
///
/// `long a[2][3]` resolves to nested `Array`s, but DDS-XTypes 7.4.3.4 makes it one array
/// of the base type: a single frame over all elements, not one per dimension. Every
/// backend needs the dimensions flat to emit that, so the walk lives here.
pub fn flatten_array(ty: &ResolvedType) -> (Vec<u32>, &ResolvedType) {
    let mut dims = Vec::new();
    let mut cur = ty;
    while let ResolvedType::Array { element, size } = cur {
        dims.push(*size);
        cur = element;
    }
    (dims, cur)
}
