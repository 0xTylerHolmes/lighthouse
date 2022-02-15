pub mod impls;
mod merkle_hasher;
mod merkleize_padded;
mod merkleize_standard;

pub use merkle_hasher::{Error, MerkleHasher};
pub use merkleize_padded::merkleize_padded;
pub use merkleize_standard::merkleize_standard;

use eth2_hashing::{hash_fixed, ZERO_HASHES, ZERO_HASHES_MAX_INDEX};
use smallvec::SmallVec;

pub const BYTES_PER_CHUNK: usize = 32;
pub const HASHSIZE: usize = 32;
pub const MERKLE_HASH_CHUNK: usize = 2 * BYTES_PER_CHUNK;
pub const MAX_UNION_SELECTOR: u8 = 127;

pub type Hash256 = ethereum_types::H256;
pub type PackedEncoding = SmallVec<[u8; BYTES_PER_CHUNK]>;

/// Convenience method for `MerkleHasher` which also provides some fast-paths for small trees.
///
/// `minimum_leaf_count` will only be used if it is greater than or equal to the minimum number of leaves that can be created from `bytes`.
pub fn merkle_root(bytes: &[u8], minimum_leaf_count: usize) -> Hash256 {
    let leaves = std::cmp::max(
        (bytes.len() + (HASHSIZE - 1)) / HASHSIZE,
        minimum_leaf_count,
    );

    if leaves == 0 {
        // If there are no bytes then the hash is always zero.
        Hash256::zero()
    } else if leaves == 1 {
        // If there is only one leaf, the hash is always those leaf bytes padded out to 32-bytes.
        let mut hash = [0; HASHSIZE];
        hash[0..bytes.len()].copy_from_slice(bytes);
        Hash256::from_slice(&hash)
    } else if leaves == 2 {
        // If there are only two leaves (this is common with BLS pubkeys), we can avoid some
        // overhead with `MerkleHasher` and just do a simple 3-node tree here.
        let mut leaves = [0; HASHSIZE * 2];
        leaves[0..bytes.len()].copy_from_slice(bytes);

        Hash256::from_slice(&hash_fixed(&leaves))
    } else {
        // If there are 3 or more leaves, use `MerkleHasher`.
        let mut hasher = MerkleHasher::with_leaves(leaves);
        hasher
            .write(bytes)
            .expect("the number of leaves is adequate for the number of bytes");
        hasher
            .finish()
            .expect("the number of leaves is adequate for the number of bytes")
    }
}

/// Returns the node created by hashing `root` and `length`.
///
/// Used in `TreeHash` for inserting the length of a list above it's root.
pub fn mix_in_length(root: &Hash256, length: usize) -> Hash256 {
    let usize_len = std::mem::size_of::<usize>();

    let mut length_bytes = [0; BYTES_PER_CHUNK];
    length_bytes[0..usize_len].copy_from_slice(&length.to_le_bytes());

    Hash256::from_slice(&eth2_hashing::hash32_concat(root.as_bytes(), &length_bytes)[..])
}

/// Returns `Some(root)` created by hashing `root` and `selector`, if `selector <=
/// MAX_UNION_SELECTOR`. Otherwise, returns `None`.
///
/// Used in `TreeHash` for the "union" type.
///
/// ## Specification
///
/// ```ignore,text
/// mix_in_selector: Given a Merkle root root and a type selector selector ("uint256" little-endian
/// serialization) return hash(root + selector).
/// ```
///
/// https://github.com/ethereum/consensus-specs/blob/v1.1.0-beta.3/ssz/simple-serialize.md#union
pub fn mix_in_selector(root: &Hash256, selector: u8) -> Option<Hash256> {
    if selector > MAX_UNION_SELECTOR {
        return None;
    }

    let mut chunk = [0; BYTES_PER_CHUNK];
    chunk[0] = selector;

    let root = eth2_hashing::hash32_concat(root.as_bytes(), &chunk);
    Some(Hash256::from_slice(&root))
}

/// Returns a cached padding node for a given height.
fn get_zero_hash(height: usize) -> &'static [u8] {
    if height <= ZERO_HASHES_MAX_INDEX {
        &ZERO_HASHES[height]
    } else {
        panic!("Tree exceeds MAX_TREE_DEPTH of {}", ZERO_HASHES_MAX_INDEX)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum TreeHashType {
    Basic,
    Vector,
    List,
    Container,
}

pub trait TreeHash {
    fn tree_hash_type() -> TreeHashType;

    fn tree_hash_packed_encoding(&self) -> PackedEncoding;

    fn tree_hash_packing_factor() -> usize;

    fn tree_hash_root(&self) -> Hash256;
}

/// Punch through references.
impl<'a, T> TreeHash for &'a T
where
    T: TreeHash,
{
    fn tree_hash_type() -> TreeHashType {
        T::tree_hash_type()
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        T::tree_hash_packed_encoding(*self)
    }

    fn tree_hash_packing_factor() -> usize {
        T::tree_hash_packing_factor()
    }

    fn tree_hash_root(&self) -> Hash256 {
        T::tree_hash_root(*self)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn mix_length() {
        let hash = {
            let mut preimage = vec![42; BYTES_PER_CHUNK];
            preimage.append(&mut vec![42]);
            preimage.append(&mut vec![0; BYTES_PER_CHUNK - 1]);
            eth2_hashing::hash(&preimage)
        };

        assert_eq!(
            mix_in_length(&Hash256::from_slice(&[42; BYTES_PER_CHUNK]), 42).as_bytes(),
            &hash[..]
        );
    }
}
