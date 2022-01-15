#![feature(exit_status_error)]
#![feature(option_result_contains)]
#![feature(associated_type_defaults)]
#![feature(associated_type_bounds)]
#![feature(generic_associated_types)]
#![feature(exact_size_is_empty)]
#![feature(result_cloned)]
#![feature(async_closure)]
#![feature(async_stream)]
#![feature(type_alias_impl_trait)]
#![feature(default_free_fn)]
#![feature(trait_alias)]

pub mod actions;
pub mod anyhow;
pub mod archive;
pub mod env;
pub mod extensions;
pub mod future;
pub mod github;
pub mod goodie;
pub mod goodies;
pub mod io;
pub mod models;
pub mod platform;
pub mod program;
pub mod programs;
pub mod pure_evil;
pub mod runner;
pub mod serde;

pub mod prelude {
    pub type Result<T = ()> = anyhow::Result<T>;
    pub use anyhow::anyhow;
    pub use anyhow::bail;
    pub use anyhow::ensure;
    // pub use anyhow::Context;
    pub use argh::FromArgs;
    pub use async_trait::async_trait;
    pub use bytes::Bytes;
    pub use derive_more::Display;
    pub use futures_util::future::BoxFuture;
    pub use futures_util::stream::BoxStream;
    pub use futures_util::AsyncWrite;
    pub use futures_util::FutureExt;
    pub use futures_util::StreamExt;
    pub use futures_util::TryFuture;
    pub use futures_util::TryFutureExt;
    pub use futures_util::TryStream;
    pub use futures_util::TryStreamExt;
    pub use ifmt::iformat;
    pub use ifmt::iprintln;
    pub use itertools::Itertools;
    pub use octocrab::Octocrab;
    pub use path_absolutize::*;
    pub use platforms::target::Arch;
    pub use platforms::target::OS;
    pub use semver::Version;
    pub use serde::Deserialize;
    pub use serde::Serialize;
    pub use snafu::Snafu;
    pub use std::borrow::Borrow;
    pub use std::borrow::Cow;
    pub use std::collections::BTreeMap;
    pub use std::default::default;
    pub use std::ffi::OsStr;
    pub use std::ffi::OsString;
    pub use std::fmt::Display;
    pub use std::future::ready;
    pub use std::future::Future;
    pub use std::io::Read;
    pub use std::io::Seek;
    pub use std::iter::once;
    pub use std::iter::FromIterator;
    pub use std::path::Path;
    pub use std::path::PathBuf;
    pub use std::stream::Stream;
    pub use std::sync::Arc;
    pub use tokio::io::AsyncWriteExt;
    pub use tokio::process::Command;
    pub use url::Url;
    pub use uuid::Uuid;

    pub use crate::EMPTY_REQUEST_BODY;

    pub use crate::anyhow::ResultExt;
    pub use crate::extensions::command::CommandExt;
    pub use crate::extensions::iterator::TryIteratorExt;
    pub use crate::extensions::output::OutputExt;
    pub use crate::extensions::path::PathExt;
    pub use crate::github::RepoPointer;
    pub use crate::goodie::Goodie;
    pub use crate::program::Program;
    pub use crate::program::ProgramExt;
    pub use crate::program::Shell;

    pub fn into<T, U>(u: U) -> T
    where U: Into<T> {
        u.into()
    }
}

/// `None` that is used to represent an empty request body in calls `octocrab`.
pub const EMPTY_REQUEST_BODY: Option<&()> = None;

use prelude::*;

use ::anyhow::Context;

/// Looks up a free port in the IANA private or dynamic port range.
pub fn get_free_port() -> Result<u16> {
    port_check::free_local_port_in_range(49152, 65535).context("Failed to find a free local port.")
}
