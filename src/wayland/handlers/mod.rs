//! sctk handler trait impls for [`super::App`], split by protocol topic.
//! All impls are deliberately thin — state transitions live on `App` itself or
//! in [`crate::dock`].

mod compositor;
mod data_device;
mod layer;
mod output;
mod pointer;
mod registry;
mod seat;
mod shm;
