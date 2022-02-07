#![feature(async_closure)]
#![feature(exit_status_error)]
#![feature(generic_associated_types)]
#![feature(associated_type_bounds)]
#![feature(option_result_contains)]
#![feature(result_flattening)]
#![feature(async_stream)]
#![feature(default_free_fn)]
#![feature(map_first_last)]

pub use ide_ci::prelude;
use ide_ci::prelude::*;

use anyhow::Context;
use enso_build::args::Args;
use enso_build::args::BuildKind;
use enso_build::args::WhatToDo;
use enso_build::enso::BuiltEnso;
use enso_build::enso::IrCaches;
use enso_build::paths::ComponentPaths;
use enso_build::paths::Paths;
use enso_build::{paths, retrieve_github_access_token};
use enso_build::setup_octocrab;
use enso_build::version::Versions;
use ide_ci::extensions::path::PathExt;
use ide_ci::future::AsyncPolicy;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::goodies::graalvm;
use ide_ci::goodies::sbt;
use ide_ci::models::config::RepoContext;
use ide_ci::program::with_cwd::WithCwd;
use ide_ci::programs::git::Git;
use ide_ci::programs::Flatc;
use ide_ci::programs::Sbt;
use platforms::TARGET_ARCH;
use platforms::TARGET_OS;
use sysinfo::SystemExt;

const FLATC_VERSION: Version = Version::new(1, 12, 0);
const GRAAL_VERSION: Version = Version::new(21, 1, 0);
const GRAAL_JAVA_VERSION: graalvm::JavaVersion = graalvm::JavaVersion::Java11;
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


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Initial environment:");

    // We want arg parsing to be the very first thing, so when user types wrong arguments, the error
    // diagnostics will be first and only thing that is output.
    let args: Args = argh::from_env();

    for (key, value) in std::env::vars() {
        // The below should not be needed - any secrets should be passed to us from the GitHub
        // Actions as secrets already. However, as a failsafe, we'll mask anythinh that looks
        // secretive.
        if key.contains("SECRET") || key.contains("TOKEN") || key.contains("KEY") {
            ide_ci::actions::workflow::mask_value(&value);
        }

        iprintln!("\t{key}={value}");
    }

    let mut config = match args.kind {
        BuildKind::Dev => LOCAL,
        BuildKind::Nightly => NIGHTLY,
    };
    if matches!(args.command, WhatToDo::Upload(_)) || args.bundle.contains(&true) {
        config.build_bundles = true;
    }

    let octocrab = setup_octocrab()?;
    let enso_root = args.repository.clone();
    println!("Repository location: {}", enso_root.display());


    let repo = ide_ci::actions::env::repository()
        .unwrap_or(RepoContext { owner: "enso-org".into(), name: "ci-build".into() });

    println!("Deciding on version to target.");
    let changelog_path = enso_build::paths::root_to_changelog(&enso_root);
    let version = if let Ok(version) = enso_build::version::version_from_legacy_repo(&enso_root) {
        println!("Using legacy version override from build.sbt: {}", version);
        version
    } else {
        Version {
            pre: match args.kind {
                BuildKind::Dev => Versions::local_prerelease()?,
                BuildKind::Nightly => Versions::nightly_prerelease(&octocrab, &repo).await?,
            },
            ..enso_build::version::base_version(&changelog_path)?
        }
    };

    let versions = Versions::new(version);
    versions.publish()?;
    println!("Target version: {versions:?}.");
    let paths = Paths::new_version(&enso_root, versions.version.clone())?;

    match args.command {
        WhatToDo::Prepare(_) => {
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
        WhatToDo::Finish(_) => {
            let release_id = enso_build::env::release_id()?;
            println!("Looking for release with id {release_id} on github.");
            let release = repo.repos(&octocrab).releases().get_by_id(release_id).await?;
            println!("Found the target release, will publish it.");
            repo.repos(&octocrab).releases().update(release.id.0).draft(false).send().await?;
            iprintln!("Done. Release URL: {release.url}");
            return Ok(());
        }
        // We don't use catch-all `_` arm, as we want to consider this point each time a new variant
        // is added.
        WhatToDo::Build(_) | WhatToDo::Upload(_) | WhatToDo::Run(_) => {}
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

    // Setup GraalVM
    let graalvm = graalvm::GraalVM {
        client:        &octocrab,
        graal_version: &GRAAL_VERSION,
        java_version:  GRAAL_JAVA_VERSION,
        os:            TARGET_OS,
        arch:          TARGET_ARCH,
    };
    goodies.require(&graalvm).await?;
    graalvm::Gu.require_present().await?;
    graalvm::Gu.cmd()?.args(["install", "native-image"]).status().await?.exit_ok()?;

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

    if let WhatToDo::Run(run) = args.command {
        return match run.command_pieces.as_slice() {
            [head, tail @ ..] => {
                let mut child = std::process::Command::new(head)
                    .args(tail)
                    .current_dir(paths.repo_root)
                    .spawn()?;
                child.wait()?.exit_ok()?;
                Ok(())
            }
            args => bail!("Invalid argument list for a command to be run: {:?}", args),
        };
    }


    let git = Git::new(&enso_root);
    if config.clean_repo {
        git.clean_xfd().await?;
        let lib_src = PathBuf::from_iter(["distribution", "lib"]);
        git.args(["checkout"])?.arg(lib_src).run_ok().await?;
    }

    // Setup Tests on Windows
    if TARGET_OS == OS::Windows {
        std::env::set_var("CI_TEST_TIMEFACTOR", "2");
        std::env::set_var("CI_TEST_FLAKY_ENABLE", "true");
    }

    //



    //



    // Disable TCP/UDP Offloading


    // Install Dependencies of the Simple Library Server
    ide_ci::programs::Npm
        .install(enso_root.join_many(["tools", "simple-library-server"]))?
        .status()
        .await?
        .exit_ok()?;

    // Download Project Template Files
    let client = reqwest::Client::new();
    download_project_templates(client.clone(), enso_root.clone()).await?;



    let sbt = WithCwd::new(Sbt, &enso_root);


    let mut system = sysinfo::System::new();
    system.refresh_memory();
    dbg!(system.total_memory());

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

    if system.total_memory() > 10_000_000 {
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



    if config.build_bundles {
        // Launcher bundle
        let bundles = create_bundles(&paths).await?;

        if matches!(args.command, WhatToDo::Upload(_)) {
            // Make packages.
            let packages = create_packages(&paths).await?;

            let release_id = enso_build::env::release_id()?;
            let repo_handler = repo.repos(&octocrab);

            // let release_name = format!("Enso {}", paths.triple.version);
            let tag_name = versions.to_string();

            let releases_handler = repo_handler.releases();
            // let triple = paths.triple.clone();
            let release = releases_handler
                .get_by_id(release_id)
                .await
                .context(format!("Failed to find release by tag {tag_name}."))?;

            let client = ide_ci::github::create_client(retrieve_github_access_token()?)?;
            for package in packages {
                ide_ci::github::release::upload_asset(&repo, &client, release.id, package).await?;
            }
            for bundle in bundles {
                ide_ci::github::release::upload_asset(&repo, &client, release.id, bundle).await?;
            }
        }
    } else {
        // Perhaps won't be needed with the new artifact API.
        package_component(&paths.engine).await?;
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
}
