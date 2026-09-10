mod simple_shuffle;

use crate::Deployment;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoutingStrategy {
    #[default]
    SimpleShuffle,
}

impl RoutingStrategy {
    pub fn select<'a>(&self, candidates: &[&'a Deployment]) -> Option<&'a Deployment> {
        match self {
            RoutingStrategy::SimpleShuffle => simple_shuffle::select(candidates),
        }
    }
}
