//! Storage of sequence blocks.

mod sequences_repository;

pub(crate) use sequences_repository::insert_block;
pub(crate) use sequences_repository::reserve_block;
