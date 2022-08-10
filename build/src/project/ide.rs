use crate::prelude::*;

pub mod packed;
pub mod unpacked;

pub use packed::Ide as Packed;
pub use unpacked::BuildInput;
pub use unpacked::Ide as Unpacked;

pub enum Artifact {
    Packed(packed::Artifact),
    Unpacked(unpacked::Artifact),
}



//
// pub trait IsIdeTarget {}
//
// // pub trait IsIdeTargetExt: IsIdeTarget {
// //     fn start_unpacked(&self, extra_ide_options: impl IntoIterator<Item: AsRef<OsStr>>) ->
// Command // {         let application_path = self.unpacked.join(&self.unpacked_executable);
// //         let mut command = if TARGET_OS == OS::MacOS {
// //             let mut ret = Command::new("open");
// //             ret.arg(application_path);
// //             ret
// //         } else {
// //             Command::new(application_path)
// //         };
// //         command.args(extra_ide_options);
// //         command
// //     }
// // }
//
pub trait IsIdeArtifact {
    fn unpacked_executable(&self) -> PathBuf;
    // fn upload_artifacts_job(&self) -> BoxFuture<Result>;
}

impl IsIdeArtifact for unpacked::Artifact {
    fn unpacked_executable(&self) -> PathBuf {
        self.unpacked.join(&self.unpacked_executable)
    }
    //
    // fn upload_artifacts_job(&self) -> BoxFuture<Result> {
    //     todo!()
    // }
}

impl IsIdeArtifact for packed::Artifact {
    fn unpacked_executable(&self) -> PathBuf {
        self.unpacked.unpacked_executable()
    }
}

impl IsIdeArtifact for Artifact {
    fn unpacked_executable(&self) -> PathBuf {
        match self {
            Artifact::Packed(artifact) => artifact.unpacked_executable(),
            Artifact::Unpacked(artifact) => artifact.unpacked_executable(),
        }
    }
}

pub trait IsIdeArtifactExt: IsIdeArtifact {
    fn start_unpacked(&self, extra_ide_options: impl IntoIterator<Item: AsRef<OsStr>>) -> Command {
        let application_path = self.unpacked_executable();
        let mut command = if TARGET_OS == OS::MacOS {
            let mut ret = Command::new("open");
            ret.arg(application_path);
            ret
        } else {
            Command::new(application_path)
        };
        command.args(extra_ide_options);
        command
    }
}

impl<T> IsIdeArtifactExt for T where T: IsIdeArtifact {}

// impl IsIdeArtifact
//
//     fn upload_as_ci_artifact(&self) -> BoxFuture<Result> {
//         async move {
//             if is_in_env() {
//                 self.upload_artifacts_job().await?;
//             } else {
//                 info!("Not in the CI environment, will not upload the artifacts.")
//             }
//             Ok(())
//         }
//     }
// }
