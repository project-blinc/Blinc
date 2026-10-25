//! Join native group fragments before batch-local indices are dispatched.
use super::{ImagePaintOrder, LayerCommand, PrimitiveBatch};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupId {
    Node(u64),
    Local(u64),
}

#[derive(Clone)]
pub struct Capture {
    pub id: GroupId,
    pub foreground_owner: bool,
    pub background: Range<usize>,
    pub foreground: Range<usize>,
}

#[derive(Clone, Default)]
pub struct CrossStreamGroups {
    pub captures: Vec<Capture>,
    pub resolved: bool,
    pub text_nodes: HashSet<u64>,
    pub svg_nodes: HashSet<u64>,
    pub image_nodes: HashSet<u64>,
}

#[derive(Clone, Copy, Default)]
struct Cursor {
    primitive: usize,
    vertex: usize,
    index: usize,
}

enum Node {
    Run(bool, Cursor, Cursor),
    Image(bool, usize),
    Layer(LayerCommand, Option<(GroupId, bool)>, Vec<Node>),
    Command(LayerCommand),
    Group(GroupId),
}

fn parse(batch: &PrimitiveBatch, foreground: bool, captures: &[Capture]) -> Vec<Node> {
    let (primitives, paths, commands, images) = if foreground {
        (
            &batch.foreground_primitives,
            &batch.foreground_paths,
            &batch.foreground_layer_commands,
            &batch.foreground_dynamic_images,
        )
    } else {
        (
            &batch.primitives,
            &batch.paths,
            &batch.layer_commands,
            &batch.dynamic_images,
        )
    };
    let markers: HashMap<_, _> = captures
        .iter()
        .map(|capture| {
            let span = if foreground {
                &capture.foreground
            } else {
                &capture.background
            };
            (span.start, (capture.id, capture.foreground_owner))
        })
        .collect();
    let end = Cursor {
        primitive: primitives.len(),
        vertex: paths.vertices.len(),
        index: paths.indices.len(),
    };
    let mut cursor = Cursor::default();
    let mut stack = vec![Vec::new()];
    let mut layers = Vec::new();
    let run = |nodes: &mut Vec<Node>, from: Cursor, to: Cursor| {
        if to.primitive > from.primitive || to.index > from.index {
            nodes.push(Node::Run(foreground, from, to));
        }
    };
    let mut images = images
        .iter()
        .enumerate()
        .filter_map(|(i, image)| image.paint_order.map(|order| (i, order)))
        .peekable();
    for command in 0..=commands.len() {
        while images
            .peek()
            .is_some_and(|(_, order)| order.layer_command_count <= command)
        {
            let (index, order) = images.next().unwrap();
            let next = Cursor {
                primitive: order.primitive_index,
                vertex: order.path_vertex_index,
                index: order.path_index_count,
            };
            run(stack.last_mut().unwrap(), cursor, next);
            stack
                .last_mut()
                .unwrap()
                .push(Node::Image(foreground, index));
            cursor = next;
        }
        let Some(entry) = commands.get(command) else {
            break;
        };
        let next = Cursor {
            primitive: entry.primitive_index,
            vertex: entry.path_vertex_index,
            index: entry.path_index_count,
        };
        run(stack.last_mut().unwrap(), cursor, next);
        cursor = next;
        match &entry.command {
            LayerCommand::Push { .. } => {
                layers.push((entry.command.clone(), markers.get(&command).copied()));
                stack.push(Vec::new());
            }
            LayerCommand::Pop if stack.len() > 1 => {
                let children = stack.pop().unwrap();
                let (config, marker) = layers.pop().unwrap();
                stack
                    .last_mut()
                    .unwrap()
                    .push(Node::Layer(config, marker, children));
            }
            LayerCommand::Pop => {}
            command => stack
                .last_mut()
                .unwrap()
                .push(Node::Command(command.clone())),
        }
    }
    run(stack.last_mut().unwrap(), cursor, end);
    while stack.len() > 1 {
        let children = stack.pop().unwrap();
        let (config, marker) = layers.pop().unwrap();
        stack
            .last_mut()
            .unwrap()
            .push(Node::Layer(config, marker, children));
    }
    stack.pop().unwrap()
}

struct Group {
    command: LayerCommand,
    background: Vec<Node>,
    foreground: Vec<Node>,
}

fn collect(
    nodes: Vec<Node>,
    foreground: bool,
    groups: &mut HashMap<GroupId, Group>,
    anchors: &mut HashSet<GroupId>,
) -> Vec<Node> {
    let mut output = Vec::new();
    for node in nodes {
        match node {
            Node::Layer(command, Some((id, owner)), children) => {
                let children = collect(children, foreground, groups, anchors);
                let group = groups.entry(id).or_insert_with(|| Group {
                    command,
                    background: Vec::new(),
                    foreground: Vec::new(),
                });
                if foreground {
                    group.foreground.extend(children)
                } else {
                    group.background.extend(children)
                }
                if owner == foreground && anchors.insert(id) {
                    output.push(Node::Group(id));
                }
            }
            Node::Layer(command, None, children) => {
                output.push(Node::Layer(
                    command,
                    None,
                    collect(children, foreground, groups, anchors),
                ));
            }
            node => output.push(node),
        }
    }
    output
}

fn emit(
    nodes: &[Node],
    source: &mut PrimitiveBatch,
    groups: &HashMap<GroupId, Group>,
    output: &mut PrimitiveBatch,
    foreground: bool,
    text: &mut Option<&mut HashMap<u64, PrimitiveBatch>>,
) {
    let command = |output: &mut PrimitiveBatch, command| {
        if foreground {
            output.push_foreground_layer_command(command)
        } else {
            output.push_layer_command(command)
        }
    };
    for node in nodes {
        match node {
            Node::Run(stream, start, end) => {
                let (primitives, paths) = if *stream {
                    (&source.foreground_primitives, &source.foreground_paths)
                } else {
                    (&source.primitives, &source.paths)
                };
                let (dest, dest_paths) = if foreground {
                    (
                        &mut output.foreground_primitives,
                        &mut output.foreground_paths,
                    )
                } else {
                    (&mut output.primitives, &mut output.paths)
                };
                dest.extend_from_slice(&primitives[start.primitive..end.primitive]);
                let base = dest_paths.vertices.len() as u32;
                dest_paths
                    .vertices
                    .extend_from_slice(&paths.vertices[start.vertex..end.vertex]);
                dest_paths.indices.extend(
                    paths.indices[start.index..end.index]
                        .iter()
                        .map(|i| i - start.vertex as u32 + base),
                );
            }
            Node::Image(stream, index) => {
                let image = if *stream {
                    &mut source.foreground_dynamic_images[*index]
                } else {
                    &mut source.dynamic_images[*index]
                };
                let data = std::mem::take(&mut image.data);
                let mut image = image.clone();
                image.data = data;
                let (primitives, paths, commands, images) = if foreground {
                    (
                        &output.foreground_primitives,
                        &output.foreground_paths,
                        &output.foreground_layer_commands,
                        &mut output.foreground_dynamic_images,
                    )
                } else {
                    (
                        &output.primitives,
                        &output.paths,
                        &output.layer_commands,
                        &mut output.dynamic_images,
                    )
                };
                image.paint_order = Some(ImagePaintOrder {
                    primitive_index: primitives.len(),
                    path_vertex_index: paths.vertices.len(),
                    path_index_count: paths.indices.len(),
                    layer_command_count: commands.len(),
                });
                images.push(image);
            }
            Node::Layer(config, None, children) => {
                command(output, config.clone());
                emit(children, source, groups, output, foreground, text);
                command(output, LayerCommand::Pop);
            }
            Node::Group(id) => {
                let group = &groups[id];
                command(output, group.command.clone());
                emit(&group.background, source, groups, output, foreground, text);
                emit(&group.foreground, source, groups, output, foreground, text);
                command(output, LayerCommand::Pop);
            }
            Node::Command(
                LayerCommand::DeferredText { id, opacity }
                | LayerCommand::DeferredSvg { id, opacity }
                | LayerCommand::DeferredImage { id, opacity },
            ) if text.is_some() => {
                if let Some(mut batch) = text.as_mut().unwrap().remove(id) {
                    debug_assert!(batch.glyphs.is_empty());
                    let shift = source.aux_data.len() as f32;
                    for primitive in &mut batch.primitives {
                        primitive.rebase_aux(shift);
                        primitive.color[3] *= opacity;
                    }
                    source.aux_data.extend(batch.aux_data);
                    if foreground {
                        output.foreground_primitives.extend(batch.primitives);
                    } else {
                        output.primitives.extend(batch.primitives);
                    }
                    for mut image in batch.dynamic_images {
                        image.opacity *= opacity;
                        let (primitives, paths, commands, images) = if foreground {
                            (
                                &output.foreground_primitives,
                                &output.foreground_paths,
                                &output.foreground_layer_commands,
                                &mut output.foreground_dynamic_images,
                            )
                        } else {
                            (
                                &output.primitives,
                                &output.paths,
                                &output.layer_commands,
                                &mut output.dynamic_images,
                            )
                        };
                        image.paint_order = Some(ImagePaintOrder {
                            primitive_index: primitives.len(),
                            path_vertex_index: paths.vertices.len(),
                            path_index_count: paths.indices.len(),
                            layer_command_count: commands.len(),
                        });
                        images.push(image);
                    }
                }
            }
            Node::Command(cmd) => command(output, cmd.clone()),
            Node::Layer(_, Some(_), _) => unreachable!(),
        }
    }
}

impl PrimitiveBatch {
    pub fn has_cross_stream_groups(&self) -> bool {
        self.cross_stream_groups.resolved || !self.cross_stream_groups.captures.is_empty()
    }

    /// Rebase captured foreground draws into their shared group's ordered owner.
    pub fn resolve_cross_stream_groups(&mut self) {
        if self.cross_stream_groups.captures.is_empty() {
            return;
        }
        self.resolve_native_streams(None);
    }

    /// Replace deferred text markers with prepared native primitives and owned aux rows.
    pub fn resolve_deferred_text(&mut self, text: &mut HashMap<u64, PrimitiveBatch>) {
        self.resolve_deferred_content(text);
    }

    /// Insert prepared native primitives followed by raw images at reserved node positions.
    pub fn resolve_deferred_content(&mut self, text: &mut HashMap<u64, PrimitiveBatch>) {
        if !self
            .layer_commands
            .iter()
            .chain(&self.foreground_layer_commands)
            .any(|entry| {
                matches!(
                    entry.command,
                    LayerCommand::DeferredText { .. }
                        | LayerCommand::DeferredSvg { .. }
                        | LayerCommand::DeferredImage { .. }
                )
            })
        {
            return;
        }
        self.resolve_native_streams(Some(text));
    }

    fn resolve_native_streams(&mut self, mut text: Option<&mut HashMap<u64, PrimitiveBatch>>) {
        let mut source = std::mem::take(self);
        let captures = std::mem::take(&mut source.cross_stream_groups.captures);
        let mut groups = HashMap::new();
        let mut anchors = HashSet::new();
        let background = collect(
            parse(&source, false, &captures),
            false,
            &mut groups,
            &mut anchors,
        );
        let foreground = collect(
            parse(&source, true, &captures),
            true,
            &mut groups,
            &mut anchors,
        );
        emit(&background, &mut source, &groups, self, false, &mut text);
        emit(&foreground, &mut source, &groups, self, true, &mut text);
        self.dynamic_images.extend(
            source
                .dynamic_images
                .into_iter()
                .filter(|i| i.paint_order.is_none()),
        );
        self.foreground_dynamic_images.extend(
            source
                .foreground_dynamic_images
                .into_iter()
                .filter(|i| i.paint_order.is_none()),
        );
        self.aux_data = source.aux_data;
        self.glass_primitives = source.glass_primitives;
        self.nested_glass_primitives = source.nested_glass_primitives;
        self.glyphs = source.glyphs;
        self.viewports_3d = source.viewports_3d;
        self.particle_viewports = source.particle_viewports;
        self.cross_stream_groups.resolved = true;
        self.cross_stream_groups.text_nodes = source.cross_stream_groups.text_nodes;
        self.cross_stream_groups.svg_nodes = source.cross_stream_groups.svg_nodes;
        self.cross_stream_groups.image_nodes = source.cross_stream_groups.image_nodes;
    }
}
