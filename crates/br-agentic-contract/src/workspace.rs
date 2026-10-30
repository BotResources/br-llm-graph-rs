use br_graph_contract::Fields;

use crate::error::PortError;
use crate::future::PortFuture;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileSystemMode {
    Always,
    Might,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFile {
    pub path: String,
    pub content: Vec<u8>,
}

pub trait WorkspacePort<F: Fields>: Send + Sync {
    fn load<'a>(&'a self, snapshot: &'a F)
    -> PortFuture<'a, Result<Vec<WorkspaceFile>, PortError>>;

    fn save<'a>(
        &'a self,
        snapshot: &'a F,
        files: Vec<WorkspaceFile>,
    ) -> PortFuture<'a, Result<(), PortError>>;
}

impl FileSystemMode {
    pub async fn start<F: Fields>(
        self,
        port: &dyn WorkspacePort<F>,
        snapshot: &F,
    ) -> Result<Option<Vec<WorkspaceFile>>, PortError> {
        match self {
            FileSystemMode::Never => Ok(None),
            FileSystemMode::Always => port.load(snapshot).await.map(Some),
            FileSystemMode::Might => {
                let files = port.load(snapshot).await?;
                Ok((!files.is_empty()).then_some(files))
            }
        }
    }
}

pub async fn save_files<F: Fields>(
    port: &dyn WorkspacePort<F>,
    snapshot: &F,
    files: Option<Vec<WorkspaceFile>>,
) -> Result<(), PortError> {
    match files {
        Some(files) => port.save(snapshot, files).await,
        None => Ok(()),
    }
}
