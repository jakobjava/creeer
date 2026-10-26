//! A structure's lasting change of shape, as the server tells it.

use crate::quant::Quantised;

/// A structure's lasting change of shape: nodes now elsewhere relative to node 0 (in its
/// frame; an index past its nodes is a node new to it: a member buckling splits in two),
/// its members anew if they changed, and members now broken.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plastic {
    /// Nodes moved for good: index and where they are now (1/1024 m).
    pub nodes: Vec<(u32, Quantised)>,
    /// All its members, if they changed (a member split where it buckles).
    pub members: Option<Vec<[u32; 2]>>,
    /// Members broken (in the members as they are now).
    pub broken: Vec<u32>,
    /// How many nodes it has now (G2.5: pieces that fell apart took theirs with them).
    pub node_count: u32,
    /// Its members' radii as drawn, when its members changed (G2.3: 0 a joint).
    pub radii: Option<Vec<f64>>,
}
