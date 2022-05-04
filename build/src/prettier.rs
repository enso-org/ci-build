use crate::prelude::*;

use crate::paths::generated::RepoRoot;

use ide_ci::programs::Npm;


pub fn install_and_run_prettier(repo_root: &RepoRoot, script: &str) -> BoxFuture<'static, Result> {
    let no_args: [&str; 0] = [];
    let prettier_dir = repo_root.build.prettier.as_path();
    let install_cmd = Npm.cmd().map(|mut cmd| cmd.install().current_dir(&prettier_dir).run_ok());
    let run_cmd =
        Npm.cmd().map(|mut cmd| cmd.current_dir(&prettier_dir).run(script, no_args).run_ok());

    async move {
        install_cmd?.await?;
        run_cmd?.await
    }
    .boxed()
}

pub fn check(repo_root: &RepoRoot) -> BoxFuture<'static, Result> {
    install_and_run_prettier(repo_root, "check")
}

pub fn write(repo_root: &RepoRoot) -> BoxFuture<'static, Result> {
    install_and_run_prettier(repo_root, "write")
}
