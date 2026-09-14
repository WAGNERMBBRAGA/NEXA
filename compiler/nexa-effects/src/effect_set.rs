use crate::effect_id::EffectId;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectSet {
    effects: BTreeSet<EffectId>,
}

impl EffectSet {
    pub fn new() -> Self {
        Self {
            effects: BTreeSet::new(),
        }
    }

    pub fn empty() -> Self {
        Self::new()
    }

    pub fn singleton(effect: EffectId) -> Self {
        let mut set = BTreeSet::new();
        set.insert(effect);
        Self { effects: set }
    }

    pub fn insert(&mut self, effect: EffectId) -> bool {
        self.effects.insert(effect)
    }

    pub fn contains(&self, effect: EffectId) -> bool {
        self.effects.contains(&effect)
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = EffectId> + '_ {
        self.effects.iter().copied()
    }

    pub fn union(&self, other: &EffectSet) -> EffectSet {
        EffectSet {
            effects: self.effects.union(&other.effects).copied().collect(),
        }
    }

    pub fn is_subset(&self, other: &EffectSet) -> bool {
        self.effects.is_subset(&other.effects)
    }

    pub fn difference(&self, other: &EffectSet) -> EffectSet {
        EffectSet {
            effects: self.effects.difference(&other.effects).copied().collect(),
        }
    }

    pub fn intersection(&self, other: &EffectSet) -> EffectSet {
        EffectSet {
            effects: self.effects.intersection(&other.effects).copied().collect(),
        }
    }

    pub fn into_sorted_vec(self) -> Vec<EffectId> {
        self.effects.into_iter().collect()
    }
}
