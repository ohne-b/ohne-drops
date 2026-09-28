use std::collections::{HashMap, HashSet, VecDeque};

use crate::{config::fold, domain::Drop};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IgnoreReason {
    Keyword(String),
    Precondition(String),
}

impl IgnoreReason {
    pub fn kind(&self) -> &str {
        match self {
            Self::Keyword(_) => "keyword",
            Self::Precondition(_) => "precondition",
        }
    }
    pub fn keyword(&self) -> Option<&str> {
        match self {
            Self::Keyword(v) => Some(v),
            _ => None,
        }
    }
    pub fn precondition(&self) -> Option<&str> {
        match self {
            Self::Precondition(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct DropPolicy {
    pub reasons: HashMap<String, IgnoreReason>,
    pub mineable: HashSet<String>,
    remaining: HashMap<String, u32>,
}

impl DropPolicy {
    pub fn evaluate(drops: &[Drop], keywords: &[String]) -> Self {
        Self::for_targets(drops, keywords, |_| true)
    }

    pub fn for_targets(
        drops: &[Drop],
        keywords: &[String],
        target: impl Fn(&Drop) -> bool,
    ) -> Self {
        let by_id: HashMap<_, _> = drops.iter().map(|d| (d.id.as_str(), d)).collect();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for drop in drops {
            for id in &drop.prerequisites {
                dependents.entry(id).or_default().push(&drop.id);
            }
        }
        let mut result = Self::default();
        let mut ignored = VecDeque::new();
        for drop in drops.iter().filter(|d| !d.claimed) {
            let name = fold(&drop.name);
            if let Some(keyword) = keywords
                .iter()
                .find(|word| !word.is_empty() && name.contains(&fold(word)))
            {
                result
                    .reasons
                    .insert(drop.id.clone(), IgnoreReason::Keyword(keyword.clone()));
                ignored.push_back(drop.id.as_str());
            }
        }
        while let Some(id) = ignored.pop_front() {
            for &child in dependents.get(id).into_iter().flatten() {
                if !by_id[child].claimed && !result.reasons.contains_key(child) {
                    result.reasons.insert(
                        child.to_owned(),
                        IgnoreReason::Precondition(by_id[id].name.clone()),
                    );
                    ignored.push_back(child);
                }
            }
        }

        // Resolve the dependency graph without recursion. Missing prerequisites and
        // cycles stay unresolved; a claimed prerequisite always satisfies its branch.
        let mut unresolved: HashMap<&str, usize> = drops
            .iter()
            .map(|d| (d.id.as_str(), d.prerequisites.len()))
            .collect();
        let mut ready: VecDeque<&str> = drops
            .iter()
            .filter(|d| d.claimed || d.prerequisites.is_empty())
            .map(|d| d.id.as_str())
            .collect();
        let mut valid = HashSet::new();
        while let Some(id) = ready.pop_front() {
            let drop = by_id[id];
            if valid.contains(id)
                || !drop.claimed && (!drop.watch_reward() || result.reasons.contains_key(id))
            {
                continue;
            }
            valid.insert(id);
            let remaining = if drop.claimed {
                0
            } else {
                drop.remaining_minutes().saturating_add(
                    drop.prerequisites
                        .iter()
                        .filter_map(|p| result.remaining.get(p))
                        .copied()
                        .max()
                        .unwrap_or(0),
                )
            };
            result.remaining.insert(id.to_owned(), remaining);
            for child in dependents.get(id).into_iter().flatten() {
                let count = unresolved.get_mut(child).expect("known dependent");
                *count = count.saturating_sub(1);
                if *count == 0 {
                    ready.push_back(child);
                }
            }
        }
        let mut useful: Vec<&str> = drops
            .iter()
            .filter(|d| {
                !d.claimed && !d.benefits.is_empty() && valid.contains(d.id.as_str()) && target(d)
            })
            .map(|d| d.id.as_str())
            .collect();
        while let Some(id) = useful.pop() {
            let drop = by_id[id];
            if drop.claimed || !result.mineable.insert(id.to_owned()) {
                continue;
            }
            useful.extend(drop.prerequisites.iter().map(String::as_str));
        }
        result
    }

    pub fn remaining_minutes(&self) -> u32 {
        self.mineable
            .iter()
            .filter_map(|id| self.remaining.get(id))
            .copied()
            .max()
            .unwrap_or(0)
    }
}
