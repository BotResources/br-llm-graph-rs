use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::shape::Shape;
use super::structure::{StructureEdge, StructureNode};
use crate::Target;

fn fill<'de, T, A>(slot: &mut Option<T>, field: &'static str, map: &mut A) -> Result<(), A::Error>
where
    T: Deserialize<'de>,
    A: MapAccess<'de>,
{
    if slot.is_some() {
        return Err(A::Error::duplicate_field(field));
    }
    *slot = Some(map.next_value()?);
    Ok(())
}

fn required<T, E: Error>(slot: Option<T>, field: &'static str) -> Result<T, E> {
    slot.ok_or_else(|| E::missing_field(field))
}

struct Record<T> {
    expected: &'static str,
    marker: PhantomData<T>,
}

impl<T> Record<T> {
    fn new(expected: &'static str) -> Self {
        Self {
            expected,
            marker: PhantomData,
        }
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum NodeField {
    Name,
    Kind,
    Subkind,
}

impl<'de> Visitor<'de> for Record<StructureNode> {
    type Value = StructureNode;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.expected)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StructureNode, A::Error> {
        let (mut name, mut kind, mut subkind) = (None, None, None);
        while let Some(field) = map.next_key()? {
            match field {
                NodeField::Name => fill(&mut name, "name", &mut map)?,
                NodeField::Kind => fill(&mut kind, "kind", &mut map)?,
                NodeField::Subkind => fill(&mut subkind, "subkind", &mut map)?,
            }
        }
        Ok(StructureNode {
            name: required(name, "name")?,
            kind: required(kind, "kind")?,
            subkind: required(subkind, "subkind")?,
        })
    }
}

impl<'de> Deserialize<'de> for StructureNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct(
            "StructureNode",
            &["name", "kind", "subkind"],
            Record::<StructureNode>::new("a map with name, kind and subkind"),
        )
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum EdgeField {
    Name,
    Source,
    Targets,
}

impl<'de> Visitor<'de> for Record<StructureEdge> {
    type Value = StructureEdge;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.expected)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StructureEdge, A::Error> {
        let (mut name, mut source, mut targets) = (None, None, None);
        while let Some(field) = map.next_key()? {
            match field {
                EdgeField::Name => fill(&mut name, "name", &mut map)?,
                EdgeField::Source => fill(&mut source, "source", &mut map)?,
                EdgeField::Targets => fill::<Vec<Target>, _>(&mut targets, "targets", &mut map)?,
            }
        }
        Ok(StructureEdge {
            name: required(name, "name")?,
            source: required(source, "source")?,
            targets: required(targets, "targets")?,
        })
    }
}

impl<'de> Deserialize<'de> for StructureEdge {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct(
            "StructureEdge",
            &["name", "source", "targets"],
            Record::<StructureEdge>::new("a map with name, source and targets"),
        )
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum ShapeField {
    Version,
    Entries,
    Nodes,
    Edges,
}

impl<'de> Visitor<'de> for Record<Shape> {
    type Value = Shape;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.expected)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Shape, A::Error> {
        let (mut version, mut entries, mut nodes, mut edges) = (None, None, None, None);
        while let Some(field) = map.next_key()? {
            match field {
                ShapeField::Version => fill(&mut version, "version", &mut map)?,
                ShapeField::Entries => fill(&mut entries, "entries", &mut map)?,
                ShapeField::Nodes => fill(&mut nodes, "nodes", &mut map)?,
                ShapeField::Edges => fill(&mut edges, "edges", &mut map)?,
            }
        }
        Ok(Shape {
            version: required(version, "version")?,
            entries: required(entries, "entries")?,
            nodes: required(nodes, "nodes")?,
            edges: required(edges, "edges")?,
        })
    }
}

impl<'de> Deserialize<'de> for Shape {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct(
            "Structure",
            &["version", "entries", "nodes", "edges"],
            Record::<Shape>::new("a map with version, entries, nodes and edges"),
        )
    }
}
