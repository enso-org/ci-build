#![feature(async_closure)]
#![feature(exit_status_error)]
#![feature(generic_associated_types)]
#![feature(associated_type_bounds)]
#![feature(option_result_contains)]
#![feature(result_flattening)]
#![feature(async_stream)]
#![feature(default_free_fn)]
#![feature(map_first_last)]
#![feature(bool_to_option)]

pub use ide_ci::prelude;
use ide_ci::prelude::*;

use anyhow::Context;
use enso_build::args::default_repo;
use enso_build::args::Args;
use enso_build::args::BuildKind;
use enso_build::args::WhatToDo;
use enso_build::enso::BuiltEnso;
use enso_build::enso::IrCaches;
use enso_build::get_graal_version;
use enso_build::get_java_major_version;
use enso_build::paths;
use enso_build::paths::ComponentPaths;
use enso_build::paths::Paths;
use enso_build::retrieve_github_access_token;
use enso_build::setup_octocrab;
use enso_build::version::Versions;
use ide_ci::extensions::path::PathExt;
use ide_ci::future::AsyncPolicy;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::goodies::graalvm;
use ide_ci::goodies::sbt;
use ide_ci::models::config::RepoContext;
use ide_ci::platform::default_shell;
use ide_ci::program::with_cwd::WithCwd;
use ide_ci::programs::git::Git;
use ide_ci::programs::Flatc;
use ide_ci::programs::Sbt;
use ide_ci::run_in_ci;
use platforms::TARGET_ARCH;
use platforms::TARGET_OS;
use std::env::consts::EXE_EXTENSION;
use sysinfo::SystemExt;


const FLATC_VERSION: Version = Version::new(1, 12, 0);
// const GRAAL_VERSION: Version = Version::new(21, 1, 0);
// const GRAAL_JAVA_VERSION: graalvm::JavaVersion = graalvm::JavaVersion::Java11;
const PARALLEL_ENSO_TESTS: AsyncPolicy = AsyncPolicy::Sequential;

pub async fn download_project_templates(client: reqwest::Client, enso_root: PathBuf) -> Result {
    // Download Project Template Files
    let output_base = enso_root.join("lib/scala/pkg/src/main/resources/");
    let url_base = Url::parse("https://github.com/enso-org/project-templates/raw/main/")?;
    let to_handle = [
        ("Orders", vec!["data/store_data.xlsx", "src/Main.enso"]),
        ("Restaurants", vec!["data/la_districts.csv", "data/restaurants.csv", "src/Main.enso"]),
        ("Stargazers", vec!["src/Main.enso"]),
    ];

    let mut futures = Vec::<BoxFuture<'static, Result>>::new();
    for (project_name, relative_paths) in to_handle {
        for relative_path in relative_paths {
            let relative_url_base = url_base.join(&format!("{}/", project_name))?;
            let relative_output_base = output_base.join(project_name.to_lowercase());
            let client = client.clone();
            let future = async move {
                ide_ci::io::download_relative(
                    &client,
                    &relative_url_base,
                    &relative_output_base,
                    &PathBuf::from(relative_path),
                )
                .await?;
                Ok(())
            };
            futures.push(future.boxed());
        }
    }

    let _result = ide_ci::future::try_join_all(futures, AsyncPolicy::FutureParallelism).await?;
    println!("Completed downloading templates");
    Ok(())
}

#[derive(Clone, Copy, Debug, Display, PartialEq)]
pub enum BuildMode {
    Development,
    NightlyRelease,
}

#[derive(Clone, Copy, Debug)]
pub struct BuildConfiguration {
    /// If true, repository shall be cleaned at the build start.
    ///
    /// Makes sense given that incremental builds with SBT are currently broken.
    clean_repo:            bool,
    mode:                  BuildMode,
    test_scala:            bool,
    test_standard_library: bool,
    /// Whether benchmarks are compiled.
    ///
    /// Note that this does not run the benchmarks, only ensures that they are buildable.
    benchmark_compilation: bool,
    build_js_parser:       bool,
    build_bundles:         bool,
}

impl BuildConfiguration {
    pub fn new(args: &Args) -> Self {
        let mut config = match args.kind {
            BuildKind::Dev => LOCAL,
            BuildKind::Nightly => NIGHTLY,
        };

        // Update build configuration with a custom arg overrides.
        if matches!(args.command, WhatToDo::Upload(_)) || args.bundle.contains(&true) {
            config.build_bundles = true;
        }
        config
    }
}

const LOCAL: BuildConfiguration = BuildConfiguration {
    clean_repo:            true,
    mode:                  BuildMode::Development,
    test_scala:            true,
    test_standard_library: true,
    benchmark_compilation: true,
    build_js_parser:       true,
    build_bundles:         false,
};

const NIGHTLY: BuildConfiguration = BuildConfiguration {
    clean_repo:            true,
    mode:                  BuildMode::NightlyRelease,
    test_scala:            false,
    test_standard_library: false,
    benchmark_compilation: false,
    build_js_parser:       false,
    build_bundles:         false,
};

pub async fn deduce_versions(
    octocrab: &Octocrab,
    build_kind: BuildKind,
    target_repo: Option<&RepoContext>,
    root_path: impl AsRef<Path>,
) -> Result<Versions> {
    println!("Deciding on version to target.");
    let changelog_path = enso_build::paths::root_to_changelog(&root_path);
    let version = Version {
        pre: match build_kind {
            BuildKind::Dev => Versions::local_prerelease()?,
            BuildKind::Nightly => {
                let repo = target_repo.cloned().or_else(|| default_repo()).ok_or_else(|| {
                    anyhow!(
                        "Missing target repository designation in the release mode. \
                        Please provide `--repo` option or `GITHUB_REPOSITORY` repository."
                    )
                })?;
                Versions::nightly_prerelease(octocrab, &repo).await?
            }
        },
        ..enso_build::version::base_version(&changelog_path)?
    };
    Ok(Versions::new(version))
}

#[derive(Clone, PartialEq, Debug)]
pub enum ReleaseCommand {
    Create,
    Upload,
    Publish,
}

impl TryFrom<WhatToDo> for ReleaseCommand {
    type Error = anyhow::Error;

    fn try_from(value: WhatToDo) -> Result<Self> {
        Ok(match value {
            WhatToDo::Create(_) => ReleaseCommand::Create,
            WhatToDo::Upload(_) => ReleaseCommand::Upload,
            WhatToDo::Publish(_) => ReleaseCommand::Publish,
            _ => bail!("Not a release command: {}", value),
        })
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ReleaseOperation {
    pub command: ReleaseCommand,
    pub repo:    RepoContext,
}

impl ReleaseOperation {
    pub fn new(args: &Args) -> Result<Self> {
        let command = args.command.clone().try_into()?;
        let repo = match args.repo.clone() {
            Some(repo) => repo,
            None => ide_ci::actions::env::repository()?,
        };

        Ok(Self { command, repo })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // We want arg parsing to be the very first thing, so when user types wrong arguments, the error
    // diagnostics will be first and only thing that is output.
    let args: Args = argh::from_env();

    println!("Initial environment:");
    for (key, value) in std::env::vars() {
        println!("\t{key}={value}");
    }
    println!("\n===End of the environment dump===\n");

    // Get default build configuration for a given build kind.
    let config = BuildConfiguration::new(&args);
    let octocrab = setup_octocrab()?;
    let enso_root = args.target.clone();
    println!("Repository location: {}", enso_root.display());
    let enso_root = args.target.absolutize()?.to_path_buf();
    println!("Canonical repository location: {}", enso_root.display());

    let release =
        if args.command.is_release_command() { Some(ReleaseOperation::new(&args)?) } else { None };

    let versions =
        deduce_versions(&octocrab, args.kind, release.as_ref().map(|r| &r.repo), &enso_root)
            .await?;
    versions.publish()?;
    println!("Target version: {versions:?}.");
    let paths = Paths::new_version(&enso_root, versions.version.clone())?;

    match release.as_ref() {
        Some(ReleaseOperation { command, repo }) => match command {
            ReleaseCommand::Create => {
                let commit = ide_ci::actions::env::commit()?;
                let latest_changelog_body =
                    enso_build::changelog::retrieve_unreleased_release_notes(paths.changelog())?;

                println!("Preparing release {} for commit {}", versions.version, commit);

                let release = repo
                    .repos(&octocrab)
                    .releases()
                    .create(&versions.tag())
                    .target_commitish(&commit)
                    .name(&versions.pretty_name())
                    .body(&latest_changelog_body.contents)
                    .prerelease(true)
                    .draft(true)
                    .send()
                    .await?;

                enso_build::env::emit_release_id(release.id);
                return Ok(());
            }
            ReleaseCommand::Publish => {
                let release_id = enso_build::env::release_id()?;
                println!("Looking for release with id {release_id} on github.");
                let release = repo.repos(&octocrab).releases().get_by_id(release_id).await?;
                println!("Found the target release, will publish it.");
                repo.repos(&octocrab).releases().update(release.id.0).draft(false).send().await?;
                iprintln!("Done. Release URL: {release.url}");

                paths.download_edition_file_artifact().await?;
                println!("Updating edition in the AWS S3.");
                enso_build::aws::update_manifest(repo, &paths).await?;

                return Ok(());
            }
            ReleaseCommand::Upload => {}
        },
        None => {}
    };

    if ide_ci::run_in_ci() {
        // On CI we remove IR caches. They might contain invalid or outdated data, as are using
        // engine version as part of the key. As such, any change made to engine that does not
        // change its version might break the caches.
        // See (private): https://discord.com/channels/401396655599124480/407883082310352928/939618590158630922
        ide_ci::io::remove_dir_if_exists(paths::cache_directory())?;
    }

    let git = Git::new(&enso_root);
    if config.clean_repo {
        git.clean_xfd().await?;
        let lib_src = PathBuf::from_iter(["distribution", "lib"]);
        git.args(["checkout"])?.arg(lib_src).run_ok().await?;
    }

    // Build environment preparations.
    let goodies = GoodieDatabase::new()?;

    // Building native images with Graal on Windows requires Microsoft Visual C++ Build Tools
    // available in the environment. If it is not visible, we need to add it.
    if TARGET_OS == OS::Windows && ide_ci::programs::vs::Cl.lookup().is_err() {
        ide_ci::programs::vs::apply_dev_environment().await?;
    }

    // Setup SBT
    goodies.require(&sbt::Sbt).await?;
    ide_ci::programs::Sbt.require_present().await?;

    // Other programs.
    ide_ci::programs::Git::default().require_present().await?;
    ide_ci::programs::Go.require_present().await?;
    ide_ci::programs::Cargo.require_present().await?;
    ide_ci::programs::Node.require_present().await?;
    ide_ci::programs::Npm.require_present().await?;

    // Setup Conda Environment
    // Install FlatBuffers Compiler
    // If it is not available, we require conda to install it. We should not require conda in other
    // scenarios.
    // TODO: After flatc version is bumped, it should be possible to get it without `conda`.
    //       See: https://www.pivotaltracker.com/story/show/180303547
    if let Err(e) = Flatc.require_present_at(&FLATC_VERSION).await {
        println!("Cannot find expected flatc: {}", e);
        // GitHub-hosted runner has `conda` on PATH but not things installed by it.
        // It provides `CONDA` variable pointing to the relevant location.
        if let Some(conda_path) = std::env::var_os("CONDA").map(PathBuf::from) {
            ide_ci::env::prepend_to_path(conda_path.join("bin"))?;
            if TARGET_OS == OS::Windows {
                // Not sure if it documented anywhere, but this is where installed `flatc` appears
                // on Windows.
                ide_ci::env::prepend_to_path(conda_path.join("Library").join("bin"))?;
            }
        }

        ide_ci::programs::Conda
            .call_args(["install", "-y", "--freeze-installed", "flatbuffers=1.12.0"])
            .await?;
        ide_ci::programs::Flatc.lookup()?;
    }

    let _ = paths.emit_env_to_actions(); // Ignore error: we might not be run on CI.
    println!("Build configuration: {:#?}", config);

    // Setup Tests on Windows
    if TARGET_OS == OS::Windows {
        std::env::set_var("CI_TEST_TIMEFACTOR", "2");
        std::env::set_var("CI_TEST_FLAKY_ENABLE", "true");
    }

    // if TARGET_OS == OS::Linux {
    //     let musl = ide_ci::goodies::musl::Musl;
    //     goodies.require(&musl).await?;
    // }

    let build_sbt_content = std::fs::read_to_string(paths.build_sbt())?;
    // Setup GraalVM
    let graalvm = graalvm::GraalVM {
        client:        &octocrab,
        graal_version: get_graal_version(&build_sbt_content)?,
        java_version:  get_java_major_version(&build_sbt_content)?,
        os:            TARGET_OS,
        arch:          TARGET_ARCH,
    };
    goodies.require(&graalvm).await?;
    graalvm::Gu.require_present().await?;
    graalvm::Gu.cmd()?.args(["install", "native-image"]).status().await?.exit_ok()?;

    if let WhatToDo::Run(run) = args.command {
        let mut run = run.command_pieces.iter();
        if let Some(program) = run.next() {
            println!("Spawning program {}.", program.to_str().unwrap());
            tokio::process::Command::new(program)
                .args(run)
                .current_dir(paths.repo_root)
                .spawn()?
                .wait()
                .await?
                .exit_ok()?;
        } else {
            println!("Spawning default shell.");
            default_shell().run_shell()?.current_dir(paths.repo_root).run_ok().await?;
        }
        return Ok(());
    }

    // Install Dependencies of the Simple Library Server
    ide_ci::programs::Npm
        .install(enso_root.join_many(["tools", "simple-library-server"]))?
        .status()
        .await?
        .exit_ok()?;


    // Download Project Template Files
    let client = reqwest::Client::new();
    download_project_templates(client.clone(), enso_root.clone()).await?;

    let git = Git::new(&enso_root);
    if config.clean_repo {
        git.clean_xfd().await?;
        let lib_src = PathBuf::from_iter(["distribution", "lib"]);
        git.args(["checkout"])?.arg(lib_src).run_ok().await?;
    }

    let sbt = WithCwd::new(Sbt, &enso_root);


    let mut system = sysinfo::System::new();
    system.refresh_memory();
    dbg!(system.total_memory());
    dbg!(system.available_memory());
    dbg!(system.used_memory());
    dbg!(system.free_memory());

    // Build packages.
    println!("Bootstrapping Enso project.");
    sbt.call_arg("bootstrap").await?;


    // TRANSITION: the PR that movevs changelog also removes the need (and possibility) of stdlib
    //             version updates through sbt
    if !paths.changelog().exists() {
        println!("Verifying the Stdlib Version.");
        sbt.call_arg("stdlib-version-updater/run update --no-format").await?;
    }

    if TARGET_OS != OS::Windows {
        // FIXME debug what is going on here
        sbt.call_arg("verifyLicensePackages").await?;
    }

    let github_hosted_macos_memory = 15032385;
    if system.total_memory() > github_hosted_macos_memory {
        let mut tasks = vec![
            "engine-runner/assembly",
            "launcher/buildNativeImage",
            "project-manager/buildNativeImage",
            "buildLauncherDistribution",
            "buildEngineDistribution",
            "buildProjectManagerDistribution",
        ];

        if config.benchmark_compilation {
            tasks.extend([
                "runtime/Benchmark/compile",
                "language-server/Benchmark/compile",
                "searcher/Benchmark/compile",
            ]);
        }

        let build_stuff = Sbt::concurrent_tasks(tasks);
        sbt.call_arg(format!("runtime/clean; {}", build_stuff)).await?;
    } else {
        // Compile
        sbt.call_arg("compile").await?;

        // Build the Runner & Runtime Uberjars
        sbt.call_arg("runtime/clean; engine-runner/assembly").await?;

        // Build the Launcher Native Image
        sbt.call_arg("launcher/assembly").await?;
        sbt.call_args(["--mem", "1536", "launcher/buildNativeImage"]).await?;

        // Build the PM Native Image
        sbt.call_arg("project-manager/assembly").await?;
        sbt.call_args(["--mem", "1536", "project-manager/buildNativeImage"]).await?;

        // Prepare Launcher Distribution
        //create_launcher_package(&paths)?;
        sbt.call_arg("buildLauncherDistribution").await?;

        // Prepare Engine Distribution
        sbt.call_arg("runtime/clean; buildEngineDistribution").await?;

        // Prepare Project Manager Distribution
        sbt.call_arg("buildProjectManagerDistribution").await?;

        if config.benchmark_compilation {
            // Check Runtime Benchmark Compilation
            sbt.call_arg("runtime/clean; runtime/Benchmark/compile").await?;

            // Check Language Server Benchmark Compilation
            sbt.call_arg("runtime/clean; language-server/Benchmark/compile").await?;

            // Check Searcher Benchmark Compilation
            sbt.call_arg("searcher/Benchmark/compile").await?;
        }
    }
    if config.test_scala {
        // Test Enso
        sbt.call_arg("set Global / parallelExecution := false; runtime/clean; compile; test")
            .await?;
    }

    // === Build Distribution ===
    // Build the Project Manager Native Image
    // FIXME looks like a copy-paste error

    if config.mode == BuildMode::Development {
        // docs-generator fails on Windows because it can't understand non-Unix-style paths.
        if TARGET_OS != OS::Windows {
            // Build the docs from standard library sources.
            sbt.call_arg("docs-generator/run").await?;
        }
    }

    if config.build_js_parser {
        // Build the Parser JS Bundle
        // TODO do once across the build
        // The builds are run on 3 platforms, but
        // Flatbuffer schemas are platform agnostic, so they just need to be
        // uploaded from one of the runners.
        sbt.call_arg("syntaxJS/fullOptJS").await?;
        ide_ci::io::copy_to(
            paths.target.join("scala-parser.js"),
            paths.target.join("parser-upload"),
        )?;
    }


    let enso = BuiltEnso { paths: paths.clone() };
    if config.test_standard_library {
        // Prepare Engine Test Environment
        if let Ok(gdoc_key) = std::env::var("GDOC_KEY") {
            let google_api_test_data_dir =
                enso_root.join("test").join("Google_Api_Test").join("data");
            ide_ci::io::create_dir_if_missing(&google_api_test_data_dir)?;
            std::fs::write(google_api_test_data_dir.join("secret.json"), &gdoc_key)?;
        }
        enso.run_tests(IrCaches::No, PARALLEL_ENSO_TESTS).await?;
    }

    let std_libs = paths.engine.dir.join("lib").join("Standard");
    // Compile the Standard Libraries (Unix)
    println!("Compiling standard libraries under {}", std_libs.display());
    for entry in std_libs.read_dir()? {
        let entry = entry?;
        let target = entry.path().join(paths.version().to_string());
        enso.compile_lib(target)?.run_ok().await?;
    }

    if config.test_standard_library {
        enso.run_tests(IrCaches::Yes, PARALLEL_ENSO_TESTS).await?;
    }


    // Verify License Packages in Distributions
    // FIXME apparently this does not work on Windows due to some CRLF issues?
    if config.mode == BuildMode::NightlyRelease && TARGET_OS != OS::Windows {
        /*  refversion=${{ env.ENSO_VERSION }}
            binversion=${{ env.DIST_VERSION }}
            engineversion=$(${{ env.ENGINE_DIST_DIR }}/bin/enso --version --json | jq -r '.version')
            test $binversion = $refversion || (echo "Tag version $refversion and the launcher version $binversion do not match" && false)
            test $engineversion = $refversion || (echo "Tag version $refversion and the engine version $engineversion do not match" && false)
        */


        async fn verify_generated_package(
            sbt: &impl Program,
            package: &str,
            path: impl AsRef<Path>,
        ) -> Result {
            sbt.cmd()?
                .arg(format!(
                    "enso/verifyGeneratedPackage {} {}",
                    package,
                    path.as_ref().join("THIRD-PARTY").display()
                ))
                .run_ok()
                .await
        }

        verify_generated_package(&sbt, "engine", &paths.engine.dir).await?;
        verify_generated_package(&sbt, "launcher", &paths.launcher.dir).await?;
        verify_generated_package(&sbt, "project-manager", &paths.project_manager.dir).await?;
        for libname in ["Base", "Table", "Image", "Database"] {
            verify_generated_package(
                &sbt,
                libname,
                paths
                    .engine
                    .dir
                    .join_many(["lib", "Standard"])
                    .join(libname)
                    .join(paths.version().to_string()),
            )
            .await?;
        }
    }

    // Compress the built artifacts for upload
    // The artifacts are compressed before upload to work around an error with long path handling in
    // the upload-artifact action on Windows. See: https://github.com/actions/upload-artifact/issues/240
    paths.engine.pack().await?;
    // let output_archive =
    // paths.engine.root.join(&paths.engine.name).with_appended_extension("zip"); // The artifacts
    // are compressed before upload to work around an error with long path handling in // the upload-artifact action on Windows. See: https://github.com/actions/upload-artifact/issues/240
    // SevenZip.add_cmd(&output_archive,
    // once("*"))?.current_dir(&paths.engine.root).run_ok().await?;

    let schema_dir =
        paths.repo_root.join_many(["engine", "language-server", "src", "main", "schema"]);
    let schema_files = schema_dir.read_dir()?.map(|e| e.map(|e| e.path())).collect_result()?;
    ide_ci::archive::create(paths.target.join("fbs-upload/fbs-schema.zip"), schema_files).await?;
    // ide_ci::io::copy_to(&schema_dir, paths.target.join("fbs-upload"))?;
    // ide_ci::programs::SevenZip
    //     .pack(paths.target.join("fbs-upload/fbs-schema.zip"), once(schema_dir.join("*")))
    //     .await?;


    if TARGET_OS == OS::Linux && run_in_ci() {
        paths.upload_edition_file_artifact().await?;
    }

    if config.build_bundles {
        // Launcher bundle
        let bundles = create_bundles(&paths).await?;

        match release.as_ref() {
            Some(ReleaseOperation { command, repo }) if *command == ReleaseCommand::Upload => {
                // Make packages.
                let packages = create_packages(&paths).await?;

                let release_id = enso_build::env::release_id()?;
                let repo_handler = repo.repos(&octocrab);

                let releases_handler = repo_handler.releases();
                let release = releases_handler
                    .get_by_id(release_id)
                    .await
                    .context(format!("Failed to find release by id `{release_id}` in `{repo}`."))?;

                let client = ide_ci::github::create_client(retrieve_github_access_token()?)?;
                for package in packages {
                    ide_ci::github::release::upload_asset(repo, &client, release.id, package)
                        .await?;
                }
                for bundle in bundles {
                    ide_ci::github::release::upload_asset(repo, &client, release.id, bundle)
                        .await?;
                }
            }
            _ => {
                package_component(&paths.engine).await?;
            }
        }
    }

    Ok(())
}

#[context("Failed to create a launcher distribution.")]
pub fn create_launcher_distribution(paths: &Paths) -> Result {
    paths.launcher.clear()?;
    ide_ci::io::copy_to(
        paths.repo_root.join_many(["distribution", "launcher", "THIRD-PARTY"]),
        &paths.launcher.dir,
    )?;
    ide_ci::io::copy_to(
        paths.repo_root.join("enso").with_extension(EXE_EXTENSION),
        &paths.launcher.dir.join("bin"),
    )?;
    //     IO.createDirectory(distributionRoot / "dist")
    //     IO.createDirectory(distributionRoot / "runtime")
    for filename in [".enso.portable", "README.md"] {
        ide_ci::io::copy_to(
            paths.repo_root.join_many(["distribution", "launcher", filename]),
            &paths.launcher.dir,
        )?;
    }
    Ok(())
}

pub async fn create_packages(paths: &Paths) -> Result<Vec<PathBuf>> {
    let mut ret = Vec::new();
    if paths.launcher.root.exists() {
        println!("Packaging launcher.");
        ret.push(package_component(&paths.launcher).await?);
        // IO.createDirectories(
        //     Seq("dist", "config", "runtime").map(root / "enso" / _)
        // )
    }

    if paths.engine.root.exists() {
        println!("Packaging engine.");
        ret.push(package_component(&paths.engine).await?);
    }
    Ok(ret)
}

#[context("Placing a GraalVM package under {}", target_directory.as_ref().display())]
pub async fn place_graal_under(target_directory: impl AsRef<Path>) -> Result {
    let graal_path = PathBuf::from(ide_ci::env::expect_var_os("JAVA_HOME")?);
    let graal_dirname = graal_path
        .file_name()
        .context(anyhow!("Invalid Graal Path deduced from JAVA_HOME: {}", graal_path.display()))?;
    ide_ci::io::mirror_directory(&graal_path, target_directory.as_ref().join(graal_dirname)).await
}

#[context("Placing a Enso Engine package in {}", target_engine_dir.as_ref().display())]
pub fn place_component_at(
    engine_paths: &ComponentPaths,
    target_engine_dir: impl AsRef<Path>,
) -> Result {
    ide_ci::io::copy(&engine_paths.dir, &target_engine_dir)
}

#[async_trait]
trait ComponentPathExt {
    async fn pack(&self) -> Result;
    fn clear(&self) -> Result;
}

#[async_trait]
impl ComponentPathExt for ComponentPaths {
    async fn pack(&self) -> Result {
        ide_ci::archive::create(&self.artifact_archive, [&self.dir]).await
    }
    fn clear(&self) -> Result {
        ide_ci::io::remove_dir_if_exists(&self.root)?;
        ide_ci::io::remove_file_if_exists(&self.artifact_archive)
    }
}

pub async fn create_bundle(
    paths: &Paths,
    base_component: &ComponentPaths,
    bundle: &ComponentPaths,
) -> Result {
    bundle.clear()?;
    ide_ci::io::copy(&base_component.root, &bundle.root)?;

    let bundled_engine_dir = bundle.dir.join("dist").join(paths.version().to_string());
    place_component_at(&paths.engine, &bundled_engine_dir)?;
    place_graal_under(bundle.dir.join("runtime")).await?;
    Ok(())
}

pub async fn create_bundles(paths: &Paths) -> Result<Vec<PathBuf>> {
    // Make sure that Graal has the needed optional components installed (on platforms that support
    // them).
    if TARGET_OS != OS::Windows {
        // Windows does not support sulong.
        graalvm::Gu.call_args(["install", "python", "r"]).await?;
    }

    // Launcher bundle.
    let engine_bundle =
        ComponentPaths::new(&paths.build_dist_root, "enso-bundle", "enso", &paths.triple);
    create_bundle(paths, &paths.launcher, &engine_bundle).await?;
    engine_bundle.pack().await?;


    // Project manager bundle.
    let pm_bundle = ComponentPaths::new(
        &paths.build_dist_root,
        "project-manager-bundle",
        "enso",
        &paths.triple,
    );
    create_bundle(paths, &paths.project_manager, &pm_bundle).await?;
    ide_ci::io::copy(
        paths.repo_root.join_many(["distribution", "enso.bundle.template"]),
        pm_bundle.dir.join(".enso.bundle"),
    )?;
    pm_bundle.pack().await?;
    Ok(vec![engine_bundle.artifact_archive, pm_bundle.artifact_archive])
}

pub async fn package_component(paths: &ComponentPaths) -> Result<PathBuf> {
    #[cfg(not(target_os = "windows"))]
    {
        let pattern =
            paths.dir.join_many(["bin", "*"]).with_extension(EXE_EXTENSION).display().to_string();
        for binary in glob::glob(&pattern)? {
            ide_ci::io::allow_owner_execute(binary?)?;
        }
    }

    ide_ci::archive::create(&paths.artifact_archive, [&paths.root]).await?;
    Ok(paths.artifact_archive.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use enso_build::paths::GuiPaths;
    use enso_build::paths::TargetTriple;
    use ide_ci::io::download_and_extract;
    use ide_ci::programs::Npm;
    // use regex::Regex;
    use tempfile::TempDir;

    // /// Workaround fix by wdanilo, see: https://github.com/rustwasm/wasm-pack/issues/790
    // pub fn js_workaround_patcher(code: impl Into<String>) -> Result<String> {
    //     let replacements = [
    //         (r"if \(\(typeof URL.*}\);", "return imports"),
    //         (r"if \(typeof module.*let result;", "return imports"),
    //         (r"export default init;", "export default init"),
    //     ];
    //
    //     let mut ret = code.into();
    //     for (regex, replacement) in replacements {
    //         let regex = Regex::new(regex).unwrap();
    //         ret = regex.replace_all(&ret, replacement).to_string();
    //     }
    //
    //     ret.push_str(
    //         "\nexport function after_load(w,m) { wasm = w; init.__wbindgen_wasm_module = m;}",
    //     );
    //     Ok(ret)
    // }
    //
    // pub fn patch_file(
    //     path: impl AsRef<Path>,
    //     patcher: impl FnOnce(String) -> Result<String>,
    // ) -> Result {
    //     let original_content = std::fs::read_to_string(&path)?;
    //     let patched_content = patcher(original_content)?;
    //     std::fs::write(path, patched_content)?;
    //     Ok(())
    // }

    // async fn download_js_assets(paths: &impl GuiPaths) -> Result {
    //     let workdir = paths.root().join(".assets-temp");
    //     ide_ci::io::reset_dir(&workdir)?;
    //
    //     // let ide_assets_main_zip = "ide-assets-main.zip";
    //     let ide_assets_url = "https://github.com/enso-org/ide-assets/archive/refs/heads/main.zip";
    //     // let unzipped_assets = workdir.join_many(["ide-assets-main", "content", "assets"]);
    //     // let js_lib_assets = paths.ide_desktop_lib_content().join("assets");
    //     download_and_extract(ide_assets_url, workdir).await?;
    //     Ok(())
    // }

    async fn init(paths: &impl GuiPaths) -> Result {
        if !paths.dist_build_init().exists() {
            println!("Initialization");
            println!("Installing build script dependencies.");
            Npm.cmd()?.current_dir(paths.script()).arg("install").run_ok().await?;
            ide_ci::io::create_dir_if_missing(paths.dist())?;
            std::fs::write(paths.dist_build_init(), "")?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn build_ide() -> Result {
        #[derive(Debug, Shrinkwrap)]
        pub struct GuiPathsData {
            #[shrinkwrap(main_field)]
            pub root: PathBuf,
            pub temp: TempDir,
        }

        impl GuiPaths for GuiPathsData {
            fn root(&self) -> &Path {
                &self.root
            }

            fn temp(&self) -> &Path {
                self.temp.path()
            }
        }

        let root_path = PathBuf::from("H:/NBO/enso5");
        let paths = GuiPathsData { root: root_path.clone(), temp: TempDir::new()? };
        let versions = Versions::new(Version::parse("2022.1.1-nightly.2022-02-03")?);
        let target = TargetTriple::new(versions);
        let is_dev = false;

        init(&paths).await?;

        let target_crate = "app/gui";
        let env = std::env::vars().filter(|(name, _val)| !name.starts_with("CARGO"));

        // let mut cmd = tokio::process::Command::new("wasm-pack");
        // cmd.env_remove("RUSTUP_TOOLCHAIN");
        // cmd.args([
        //     "build",
        //     "--target",
        //     "web",
        //     "--out-dir",
        //     &paths.wasm().as_os_str().to_str().unwrap(),
        //     "--out-name",
        //     "ide",
        //     target_crate,
        // ])
        // .current_dir(root_path)
        // .spawn()?
        // .wait()
        // .await?
        // .exit_ok()?;
        //
        //
        // patch_file(paths.wasm_glue(), js_workaround_patcher)?;
        // std::fs::rename(paths.wasm_main_raw(), paths.wasm_main())?;

        // if (!argv.dev) {
        //     console.log('Minimizing the WASM binary.')
        //                 await gzip(paths.wasm.main, paths.wasm.mainGz)
        //
        //     const limitMb = 4.6
        //     await checkWasmSize(paths.wasm.mainGz, limitMb)
        // }
        // Copy WASM files from temporary directory to Webpack's `dist` directory.
        // ide_ci::io::copy(paths.wasm(), paths.dist_wasm())?;



        // // JS PART
        Npm.args(["run", "install"])?.current_dir(paths.ide_desktop()).run_ok().await?;
        //download_js_assets(&paths).await?;
        enso_build::project_manager::ensure_present(paths.dist(), &target).await?;

        Npm.cmd()?.current_dir(paths.ide_desktop()).args(["run", "dist"]).run_ok().await?;

        println!("{}", paths.temp.path().display());
        std::mem::forget(paths.temp);
        Ok(())
    }
}
