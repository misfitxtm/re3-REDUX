//! Game file formats shared by GTA III and Vice City.
// fixed-size record walks read clearer with chunks_exact than as_chunks
#![allow(clippy::chunks_exact_to_as_chunks)]
pub mod carcols;
pub mod col;
pub mod combat;
pub mod datafile;
pub mod dff;
pub mod dir;
pub mod gxt;
pub mod ifp;
pub mod handling;
pub mod ped;
pub mod rw;
pub mod txd;
mod scan;
pub mod world;

pub use datafile::Game;
