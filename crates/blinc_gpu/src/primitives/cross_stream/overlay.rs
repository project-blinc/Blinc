//! Paint nested portals after their late owner's ordinary foreground content.
use super::{Group, GroupId, Node};
use std::collections::HashMap;

struct Deferred {
    order: u32,
    node: Node,
}

pub(super) fn resolve(
    background: &mut Vec<Node>,
    foreground: &mut Vec<Node>,
    groups: &mut HashMap<GroupId, Group>,
) {
    for nodes in [background, foreground] {
        let (ordinary, deferred) = extract(std::mem::take(nodes), groups, false);
        debug_assert!(deferred.is_empty());
        *nodes = ordinary;
    }
}

fn extract(
    nodes: Vec<Node>,
    groups: &mut HashMap<GroupId, Group>,
    inside_overlay: bool,
) -> (Vec<Node>, Vec<Deferred>) {
    let mut ordinary = Vec::new();
    let mut deferred = Vec::new();
    for node in nodes {
        match node {
            Node::Group(id) => {
                let mut group = groups.remove(&id).unwrap();
                let overlay = group.overlay_order.is_some();
                let (background, mut late) =
                    extract(group.background, groups, inside_overlay || overlay);
                let (foreground, late_foreground) =
                    extract(group.foreground, groups, inside_overlay || overlay);
                late.extend(late_foreground);
                group.background = background;
                group.foreground = foreground;
                if overlay {
                    // Background and foreground visits share one tree order.
                    late.sort_by_key(|entry| entry.order);
                    group
                        .foreground
                        .extend(late.into_iter().map(|entry| entry.node));
                } else {
                    // A portal escapes ordinary groups while retaining their visual state.
                    deferred.extend(late.into_iter().map(|entry| Deferred {
                        order: entry.order,
                        node: Node::Layer(group.command.clone(), None, vec![entry.node]),
                    }));
                }
                let order = group.overlay_order;
                groups.insert(id, group);
                if inside_overlay && let Some(order) = order {
                    deferred.push(Deferred {
                        order,
                        node: Node::Group(id),
                    });
                } else {
                    ordinary.push(Node::Group(id));
                }
            }
            Node::Layer(command, None, children) => {
                let (children, late) = extract(children, groups, inside_overlay);
                deferred.extend(late.into_iter().map(|entry| Deferred {
                    order: entry.order,
                    node: Node::Layer(command.clone(), None, vec![entry.node]),
                }));
                ordinary.push(Node::Layer(command, None, children));
            }
            node => ordinary.push(node),
        }
    }
    (ordinary, deferred)
}
