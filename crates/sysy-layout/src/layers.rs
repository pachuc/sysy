//! Longest-path layers and eight pairs of barycenter sweeps.

use std::collections::{BTreeMap, BTreeSet};

use sysy_core::Design;

use crate::geometry::Point;

#[derive(Clone)]
struct Arc {
    id: String,
    from: String,
    to: String,
}

pub(crate) fn paths(design: &Design) -> BTreeMap<String, Vec<String>> {
    let parents: BTreeMap<_, _> = design
        .containers
        .iter()
        .map(|container| (container.id.as_str(), container.parent.as_deref()))
        .collect();
    design
        .nodes
        .iter()
        .map(|node| (&node.id, node.container.as_deref()))
        .chain(
            design
                .containers
                .iter()
                .map(|container| (&container.id, container.parent.as_deref())),
        )
        .map(|(id, mut parent)| {
            let mut path = Vec::new();
            while let Some(ancestor) = parent {
                path.push(ancestor.to_owned());
                parent = parents[ancestor];
            }
            path.reverse();
            (id.clone(), path)
        })
        .collect()
}

fn arcs(design: &Design, paths: &BTreeMap<String, Vec<String>>) -> Vec<Arc> {
    // Container endpoints stand for all their descendant nodes. Empty
    // containers remain vertices so their connections still affect placement.
    let endpoints = |id: &str| -> Vec<String> {
        let children: Vec<_> = paths
            .iter()
            .filter(|(_, path)| path.iter().any(|part| part == id))
            .map(|(child, _)| child.clone())
            .collect();
        if children.is_empty() {
            vec![id.to_owned()]
        } else {
            children
        }
    };
    let mut arcs = Vec::new();
    for edge in &design.edges {
        for from in endpoints(&edge.from) {
            for to in endpoints(&edge.to) {
                arcs.push(Arc {
                    id: edge.id.clone(),
                    from: from.clone(),
                    to,
                });
            }
        }
    }
    arcs.sort_by(|a, b| (&a.id, &a.from, &a.to).cmp(&(&b.id, &b.from, &b.to)));
    arcs
}

fn reachable(arcs: &[Arc], from: &str, to: &str) -> bool {
    let mut pending = vec![from];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if id == to {
            return true;
        }
        if seen.insert(id) {
            pending.extend(
                arcs.iter()
                    .filter(|arc| arc.from == id)
                    .map(|arc| arc.to.as_str()),
            );
        }
    }
    false
}

fn break_cycles(arcs: &mut Vec<Arc>) {
    // Test membership in a cycle, rather than dropping an arbitrary DFS back
    // edge: the greatest edge id in a cycle must be the one removed.
    while let Some(id) = arcs
        .iter()
        .rev()
        .find(|arc| reachable(arcs, &arc.to, &arc.from))
        .map(|arc| arc.id.clone())
    {
        arcs.retain(|arc| arc.id != id);
    }
}

fn assign_layers(paths: &BTreeMap<String, Vec<String>>, arcs: &[Arc]) -> Vec<Vec<String>> {
    let mut indegree: BTreeMap<_, usize> = paths.keys().map(|id| (id.clone(), 0)).collect();
    let mut ranks: BTreeMap<_, usize> = paths.keys().map(|id| (id.clone(), 0)).collect();
    for arc in arcs {
        *indegree.get_mut(&arc.to).expect("known endpoint") += 1;
    }
    let mut ready: BTreeSet<_> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    while let Some(id) = ready.pop_first() {
        for arc in arcs.iter().filter(|arc| arc.from == id) {
            let rank = ranks[&id] + 1;
            let target = ranks.get_mut(&arc.to).expect("known endpoint");
            *target = (*target).max(rank);
            let degree = indegree.get_mut(&arc.to).expect("known endpoint");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(arc.to.clone());
            }
        }
    }
    let mut layers = vec![Vec::new(); ranks.values().max().map_or(0, |rank| rank + 1)];
    for (id, rank) in ranks {
        layers[rank].push(id);
    }
    layers
}

fn positions(layer: &[String]) -> BTreeMap<&str, f64> {
    layer
        .iter()
        .scan(0.0, |position, id| {
            let result = (id.as_str(), *position);
            *position += 1.0;
            Some(result)
        })
        .collect()
}

fn sweep(
    layer: &mut [String],
    adjacent: &[String],
    arcs: &[Arc],
    paths: &BTreeMap<String, Vec<String>>,
) {
    let neighbours = positions(adjacent);
    let original = positions(layer);
    let scores: BTreeMap<_, _> = layer
        .iter()
        .map(|id| {
            let (sum, count) = arcs
                .iter()
                .filter_map(|arc| {
                    if arc.from == *id {
                        neighbours.get(arc.to.as_str())
                    } else if arc.to == *id {
                        neighbours.get(arc.from.as_str())
                    } else {
                        None
                    }
                })
                .fold((0.0, 0.0), |(sum, count), position| {
                    (sum + position, count + 1.0)
                });
            (
                id.clone(),
                if count > 0.0 {
                    sum / count
                } else {
                    original[id.as_str()]
                },
            )
        })
        .collect();
    let mut groups: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for id in layer.iter() {
        for ancestor in &paths[id] {
            let (sum, count) = groups.entry(ancestor).or_default();
            *sum += scores[id];
            *count += 1.0;
        }
    }
    // A shared prefix gives every container a contiguous run, at every depth.
    let keys: BTreeMap<_, Vec<_>> = layer
        .iter()
        .map(|id| {
            let key = paths[id]
                .iter()
                .map(|ancestor| {
                    let (sum, count) = groups[ancestor.as_str()];
                    (sum / count, ancestor.clone())
                })
                .chain(std::iter::once((scores[id], id.clone())))
                .collect();
            (id.clone(), key)
        })
        .collect();
    layer.sort_by(|a, b| {
        keys[a]
            .iter()
            .zip(&keys[b])
            .map(|((a_score, a_id), (b_score, b_id))| {
                a_score.total_cmp(b_score).then_with(|| a_id.cmp(b_id))
            })
            .find(|order| !order.is_eq())
            .unwrap_or_else(|| a.cmp(b))
    });
}

pub(crate) fn arrange(
    design: &Design,
    all_paths: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Point> {
    let paths: BTreeMap<_, _> = all_paths
        .iter()
        .filter(|(id, _)| {
            !design
                .containers
                .iter()
                .any(|container| container.parent.as_ref() == Some(id))
                && !design
                    .nodes
                    .iter()
                    .any(|node| node.container.as_ref() == Some(id))
        })
        .map(|(id, path)| (id.clone(), path.clone()))
        .collect();
    let mut arcs = arcs(design, &paths);
    break_cycles(&mut arcs);
    let mut layers = assign_layers(&paths, &arcs);
    for _ in 0..8 {
        for index in 1..layers.len() {
            let (left, right) = layers.split_at_mut(index);
            sweep(&mut right[0], &left[index - 1], &arcs, &paths);
        }
        for index in (1..layers.len()).rev() {
            let (left, right) = layers.split_at_mut(index);
            sweep(&mut left[index - 1], &right[0], &arcs, &paths);
        }
    }
    layers
        .into_iter()
        .scan(0.0, |x, layer| {
            let column = *x;
            *x += 1.0;
            Some(layer.into_iter().scan(0.0, move |y, id| {
                let point = Point { x: column, y: *y };
                *y += 1.0;
                Some((id, point))
            }))
        })
        .flatten()
        .collect()
}
