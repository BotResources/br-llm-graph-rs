use std::collections::HashSet;

use br_graph_contract::{Fields, Graph};

use crate::{ResumeError, StartError};

pub(crate) enum NameFault {
    Empty,
    Duplicate(String),
    Unknown(String),
}

pub(crate) fn check<F: Fields>(graph: &Graph<F>, names: &[String]) -> Result<(), NameFault> {
    if names.is_empty() {
        return Err(NameFault::Empty);
    }
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name.as_str()) {
            return Err(NameFault::Duplicate(name.clone()));
        }
        if graph.node(name).is_none() {
            return Err(NameFault::Unknown(name.clone()));
        }
    }
    Ok(())
}

impl From<NameFault> for StartError {
    fn from(fault: NameFault) -> Self {
        match fault {
            NameFault::Empty => Self::NoNodes,
            NameFault::Duplicate(name) => Self::DuplicateNode { name },
            NameFault::Unknown(name) => Self::UnknownNode { name },
        }
    }
}

impl From<NameFault> for ResumeError {
    fn from(fault: NameFault) -> Self {
        match fault {
            NameFault::Empty => Self::NoNodes,
            NameFault::Duplicate(name) => Self::DuplicateNode { name },
            NameFault::Unknown(name) => Self::UnknownNode { name },
        }
    }
}
