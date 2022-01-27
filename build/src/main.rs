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
use enso_build::retrieve_github_access_token;
use enso_build::setup_octocrab;
use enso_build::version::Versions;
use filetime::FileTime;
use glob::glob;
use ide_ci::actions::workflow;
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
use std::env::consts::EXE_EXTENSION;
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
    benchmark_compilation: bool,
    build_js_parser:       bool,
}

const LOCAL: BuildConfiguration = BuildConfiguration {
    clean_repo:            true,
    mode:                  BuildMode::Development,
    test_scala:            true,
    test_standard_library: true,
    benchmark_compilation: true,
    build_js_parser:       true,
};

const NIGHTLY: BuildConfiguration = BuildConfiguration {
    clean_repo:            true,
    mode:                  BuildMode::NightlyRelease,
    test_scala:            false,
    test_standard_library: false,
    benchmark_compilation: false,
    build_js_parser:       false,
};


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Initial environment:");
    for (key, value) in std::env::vars() {
        // The below should not be needed - any secrets should be passed to us from the GitHub
        // Actions as secrets already. However, as a failsafe, we'll mask anythinh that looks
        // secretive.
        if key.contains("SECRET") || key.contains("TOKEN") || key.contains("KEY") {
            ide_ci::actions::workflow::mask_value(&value);
        }

        iprintln!("\t{key}={value}");
    }

    let args: Args = argh::from_env();
    let config = match args.kind {
        BuildKind::Dev => LOCAL,
        BuildKind::Nightly => NIGHTLY,
    };

    let octocrab = setup_octocrab()?;
    let enso_root = args.repository.clone();
    println!("Repository location: {}", enso_root.display());


    let repo = ide_ci::actions::env::repository()
        .unwrap_or(RepoContext { owner: "enso-org".into(), name: "ci-build".into() });

    println!("Deciding on version to target.");
    let changelog_path = enso_build::paths::root_to_changelog(&enso_root);
    let mut version = enso_build::version::base_version(&changelog_path)?;
    version.pre = match args.kind {
        BuildKind::Dev => Versions::local_prerelease()?,
        BuildKind::Nightly => Versions::nightly_prerelease(&octocrab, &repo).await?,
    };
    let versions = Versions::new(version);
    versions.publish()?;
    println!("Target version: {versions}.");
    let paths = Paths::new_version(&enso_root, versions.version.clone())?;

    match args.command {
        WhatToDo::Prepare => {
            let commit = ide_ci::actions::env::commit()?;
            let latest_changelog_body =
                enso_build::changelog::retrieve_unreleased_release_notes(paths.changelog())?;

            println!("Preparing release {} for commit {}", versions.version, commit);

            let release = repo
                .repos(&octocrab)
                .releases()
                .create(&versions.tag())
                .target_commitish(&commit)
                .name(&versions.to_string())
                .body(&latest_changelog_body.contents)
                .prerelease(true)
                .draft(true)
                .send()
                .await?;

            enso_build::env::emit_release_id(release.id);

            return Ok(());
        }
        WhatToDo::Finish => {
            let release_id = enso_build::env::release_id()?;
            println!("Looking for release with id {release_id} on github.");
            let release = repo.repos(&octocrab).releases().get_by_id(release_id).await?;
            println!("Found the target release, will publish it.");
            repo.repos(&octocrab).releases().update(release.id.0).draft(false).send().await?;
            iprintln!("Done. Release URL: {release.url}");
            return Ok(());
        }
        WhatToDo::Build | WhatToDo::Upload => {}
    }

    let git = Git::new(&enso_root);
    if config.clean_repo {
        git.clean_xfd().await?;
        git.args(["checkout", PathBuf::from_iter(["distribution","lib"])?.run_ok().await?;
    }

    let _ = paths.emit_env_to_actions(); // Ignore error: we might not be run on CI.
    println!("Build configuration: {:#?}", config);

    let goodies = GoodieDatabase::new()?;
    let client = reqwest::Client::new();

    // Building native images with Graal on Windows requires Microsoft Visual C++ Build Tools
    // available in the environment. If it is not visible, we need to add it.
    if TARGET_OS == OS::Windows && ide_ci::programs::vs::Cl.lookup().is_err() {
        ide_ci::programs::vs::apply_dev_environment().await?;
    }

    // Setup Tests on Windows
    if TARGET_OS == OS::Windows {
        std::env::set_var("CI_TEST_TIMEFACTOR", "2");
        std::env::set_var("CI_TEST_FLAKY_ENABLE", "true");
    }

    // ide_ci::actions::workflow::set_env("ENSO_RELEASE_MODE", args.release_mode.to_string()).ok();
    ide_ci::programs::Go.require_present().await?;
    ide_ci::programs::Cargo.require_present().await?;
    ide_ci::programs::Node.require_present().await?;
    ide_ci::programs::Npm.require_present().await?;


    // Disable TCP/UDP Offloading

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

    // Install Dependencies of the Simple Library Server
    ide_ci::programs::Npm
        .install(enso_root.join_many(["tools", "simple-library-server"]))?
        .status()
        .await?
        .exit_ok()?;

    // Download Project Template Files
    download_project_templates(client.clone(), enso_root.clone()).await?;

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

    // Setup SBT
    goodies.require(&sbt::Sbt).await?;
    ide_ci::programs::Sbt.require_present().await?;


    graalvm::Gu.cmd()?.args(["install", "native-image"]).status().await?.exit_ok()?;


    let sbt = WithCwd::new(Sbt, &enso_root);


    let mut system = sysinfo::System::new();
    system.refresh_memory();
    dbg!(system.total_memory());

    // Build packages.
    println!("Bootstrapping Enso project.");
    sbt.call_arg("bootstrap").await?;

    println!("Verifying the Stdlib Version.");
    sbt.call_arg("stdlib-version-updater/run update --no-format").await?;
    if TARGET_OS != OS::Windows {
        // FIXME debug what is going on here
        sbt.call_arg("verifyLicensePackages").await?;
    }
    // match config.mode {
    //     BuildMode::Development => {
    //         sbt.call_arg("stdlib-version-updater/run check").await?;
    //     }
    //     BuildMode::NightlyRelease => {
    //         sbt.call_arg("stdlib-version-updater/run update --no-format").await?;
    //         if TARGET_OS != OS::Windows {
    //             // FIXME debug what is going on here
    //             sbt.call_arg("verifyLicensePackages").await?;
    //         }
    //     }
    // };

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
        let test_result = sbt
            .call_arg("set Global / parallelExecution := false; runtime/clean; compile; test")
            .await;
        if let Err(err) = test_result {
            workflow::Message {
                level: workflow::MessageLevel::Error,
                text:  iformat!("Tests failed: {err}"),
            }
        } else {
            workflow::Message {
                level: workflow::MessageLevel::Notice,
                text:  iformat!("Tests were completed successfully."),
            }
        }
        .send();
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
        let target = entry.path().join(paths.triple.version.to_string());
        enso.compile_lib(target)?.run_ok().await?;
    }

    if config.test_standard_library {
        enso.run_tests(IrCaches::Yes, PARALLEL_ENSO_TESTS).await?;
    }

    if config.mode == BuildMode::NightlyRelease {
        /*  refversion=${{ env.ENSO_VERSION }}
            binversion=${{ env.DIST_VERSION }}
            engineversion=$(${{ env.ENGINE_DIST_DIR }}/bin/enso --version --json | jq -r '.version')
            test $binversion = $refversion || (echo "Tag version $refversion and the launcher version $binversion do not match" && false)
            test $engineversion = $refversion || (echo "Tag version $refversion and the engine version $engineversion do not match" && false)
        */


        // Verify License Packages in Distributions
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
                    .join(paths.triple.version.to_string())
                    .join(libname),
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

    if args.command == WhatToDo::Upload {
        let release_id = enso_build::env::release_id()?;

        // Make packages.
        let packages = create_packages(&paths).await?;

        // Launcher bundle
        let bundles = create_bundles(&paths).await?;

        let repo_handler = repo.repos(&octocrab);

        // let release_name = format!("Enso {}", paths.triple.version);
        let tag_name = paths.triple.version.to_string();

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

pub async fn create_bundles(paths: &Paths) -> Result<Vec<PathBuf>> {
    let engine_bundle =
        ComponentPaths::new(&paths.build_dist_root, "enso-bundle", "enso", &paths.triple);
    engine_bundle.clear()?;
    ide_ci::io::copy(&paths.launcher.root, &engine_bundle.root)?;

    // Install Graalpython & FastR
    if TARGET_OS != OS::Windows {
        // Windows does not support sulong.
        graalvm::Gu.call_args(["install", "python", "r"]).await?;
    }

    // Copy engine into the bundle.
    let bundled_engine_dir = engine_bundle.dir.join("dist").join(paths.triple.version.to_string());
    place_component_at(&paths.engine, &bundled_engine_dir)?;
    place_graal_under(engine_bundle.dir.join("runtime")).await?;
    engine_bundle.pack().await?;

    // Project manager bundle.
    let pm_bundle = ComponentPaths::new(
        &paths.build_dist_root,
        "project-manager-bundle",
        "enso",
        &paths.triple,
    );
    pm_bundle.clear()?;
    ide_ci::io::copy(&paths.project_manager.root, &pm_bundle.root)?;
    place_component_at(&paths.engine, &bundled_engine_dir)?;
    place_graal_under(pm_bundle.dir.join("runtime")).await?;
    ide_ci::io::copy(
        paths.repo_root.join_many(["distribution", "enso.bundle.template"]),
        pm_bundle.dir.join(".enso.bundle"),
    )?;
    pm_bundle.pack().await?;
    Ok(vec![engine_bundle.artifact_archive, pm_bundle.artifact_archive])

    // TODO similar for the Project Manager

    /*
      val pm = builtArtifact("project-manager", os, arch)
      if (pm.exists()) {
        if (os.isUNIX) {
          makeExecutable(pm / "enso" / "bin" / "project-manager")
        }

        copyEngine(os, arch, pm / "enso" / "dist")
        copyGraal(os, arch, pm / "enso" / "runtime")

        IO.copyFile(
          file("distribution/enso.bundle.template"),
          pm / "enso" / ".enso.bundle"
        )

        val archive = builtArchive("project-manager", os, arch)
        makeArchive(pm, "enso", archive)

        cleanDirectory(pm / "enso" / "dist")
        cleanDirectory(pm / "enso" / "runtime")

        log.info(s"Created $archive")
      }
    }

      */
}

pub async fn package_component(paths: &ComponentPaths) -> Result<PathBuf> {
    #[cfg(not(target_os = "windows"))]
    {
        use std::env::consts::EXE_EXTENSION;
        let pattern =
            paths.dir.join_many(["bin", "*"]).with_extension(EXE_EXTENSION).display().to_string();
        for binary in glob::glob(&pattern)? {
            ide_ci::io::allow_owner_execute(binary?);
        }
    }

    ide_ci::archive::create(&paths.artifact_archive, [&paths.root]).await?;
    Ok(paths.artifact_archive.clone())
}

#[derive(Clone, Debug)]
pub struct FragileFiles {
    sources: Vec<PathBuf>,
    classes: Vec<PathBuf>,
}

pub fn get_fragile_files(enso_root: impl AsRef<Path>) -> Result<FragileFiles> {
    let runtime_root = enso_root.as_ref().join_many(["engine", "runtime"]);
    let runtime_src = runtime_root.join_many(["src", "main", "java"]);
    let runtime_classes = runtime_root.join_many(["target", "*", "classes"]);
    let interpreter_path = ["org", "enso", "interpreter"];
    let suffixes: [&[&str]; 3] = [&["Language"], &["epb", "EpbLanguage"], &["**", "*Instrument"]];

    let get_files = |path_prefix: &Path, extension: &str| -> Result<Vec<PathBuf>> {
        let mut ret = Vec::new();
        for suffix in suffixes {
            let pattern =
                path_prefix.join_many(interpreter_path).join_many(suffix).with_extension(extension);
            println!("Searching the pattern: {}", pattern.display());
            for entry in glob(pattern.to_str().unwrap())? {
                ret.push(entry?);
            }
        }
        Ok(ret)
    };

    Ok(FragileFiles {
        sources: get_files(&runtime_src, "java")?,
        classes: get_files(&runtime_classes, "class")?,
    })
}

pub fn clear_fragile_files_smart(enso_root: impl AsRef<Path>) -> Result {
    let fragile_files = get_fragile_files(enso_root)?;

    let time_to_set = FileTime::now();
    for src_file in &fragile_files.sources {
        println!("Touching {}", src_file.display());
        filetime::set_file_mtime(src_file, time_to_set)?;
    }
    for class_file in &fragile_files.classes {
        println!("Deleting {}", class_file.display());
        ide_ci::io::remove_file_if_exists(class_file)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    use enso_build::postgres::process_lines;
    use ide_ci::extensions::path::PathExt;
    use ide_ci::github::release::upload_asset;
    use ide_ci::models::config::RepoContext;
    use ide_ci::programs::git::Git;
    use ide_ci::programs::Cmd;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::io::BufReader;

    #[tokio::test]
    #[ignore]
    async fn fgbhduiydfgiu() -> Result {
        let enso_root = r"H:\NBO\enso";
        let sbt = WithCwd::new(Sbt, &enso_root);
        sbt.cmd()?
            .arg(r"enso/verifyGeneratedPackage engine H:\NBO\enso\built-distribution\enso-engine-2022.1.1-nightly.2022-01-27-windows-amd64\enso-2022.1.1-nightly.2022-01-27\THIRD-PARTY")
            .run_ok()
            .await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_paths() -> Result {
        Ok(())
    }

    // #[tokio::test]
    // async fn run_test() -> Result {
    //     let paths = Paths::new(r"H:\NBO\enso")?;
    //     run_tests(&paths, IrCaches::No).await?;
    //     Ok(())
    // }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    #[ignore]
    async fn named_pipe_sbt() -> Result {
        use tokio::net::windows::named_pipe::ClientOptions;

        let path = PathBuf::from(r"H:\NBO\enso");
        let active_path = path.join_many(["project", "target", "active.json"]);
        let contents = std::fs::read_to_string(&active_path)?;

        #[derive(Clone, Debug, Deserialize)]
        struct Active {
            uri: String,
        }

        let active = serde_json::from_str::<Active>(&contents)?;
        if TARGET_OS == OS::Windows {
            assert!(active.uri.starts_with("local:"));
            let name = active.uri.replacen("local:", r"\\.\pipe\", 1);
            println!("Will connect to pipe {}", name);
            let pipe = ClientOptions::new().open(name)?;
            let (rx, mut tx) = tokio::io::split(pipe);
            println!("Connection established.");

            tx.write_all(r#"{ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "initializationOptions": {} } }"#.as_bytes()).await?;
            tx.write_all("\n".as_bytes()).await?;

            tx.write_all(r#"{ "jsonrpc": "2.0", "id": 2, "method": "sbt/exec", "params": {"commandLine": "bootstrap" } }"#.as_bytes(),).await?;
            tx.write_all("\n".as_bytes()).await?;

            tx.write_all(r#"{ "jsonrpc": "2.0", "id": 3, "method": "sbt/exec", "params": {"commandLine": "all buildLauncherDistribution buildEngineDistribution buildProjectManagerDistribution" } }"#.as_bytes(),).await?;
            tx.write_all("\n".as_bytes()).await?;

            tx.write_all(r#"{ "jsonrpc": "2.0", "id": 4, "method": "sbt/exec", "params": {"commandLine": "exit" } }"#.as_bytes(),).await?;
            tx.write_all("\n".as_bytes()).await?;
            drop(tx);

            println!("Sent request.");
            let mut rx = BufReader::new(rx);
            tokio::spawn(async move {
                println!("Will read.");
                loop {
                    let mut buffer = [0; 10000];
                    let count = rx.read(&mut buffer[..]).await.unwrap();
                    println!("GOT: {}", String::from_utf8_lossy(&buffer[..count]));
                }
            })
            .await?;
            // process_lines(rx, |line| println!("GOT: {}", line)).await?;
        }

        // ide_ci::programs::vs::apply_dev_environment().await?;
        // let git = Git::new(&path);
        // // git.clean_xfd().await?;
        // Sbt.cmd()?.current_dir(&path).arg("bootstrap").run_ok().await?;
        // Sbt.cmd()?.current_dir(&path).arg("all buildLauncherDistribution buildEngineDistribution
        // buildProjectManagerDistribution").run_ok().await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn good_batch_package() -> Result {
        let path = PathBuf::from(r"H:\NBO\enso");
        ide_ci::programs::vs::apply_dev_environment().await?;
        let git = Git::new(&path);
        git.clean_xfd().await?;
        Sbt.cmd()?.current_dir(&path).arg("bootstrap").run_ok().await?;
        Sbt.cmd()?.current_dir(&path).arg("all buildLauncherDistribution buildEngineDistribution buildProjectManagerDistribution").run_ok().await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn good_batch_test() -> Result {
        let path = PathBuf::from(r"H:\NBO\enso");
        // ide_ci::programs::vs::apply_dev_environment().await?;
        // let git = Git::new(&path);
        // git.clean_xfd().await?;
        // Sbt.cmd()?.current_dir(&path).arg("bootstrap").run_ok().await?;
        Sbt.cmd()?.current_dir(&path).arg("all test").run_ok().await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn interactive_sbt() -> Result {
        let paths = Paths::new(r"H:\NBO\enso")?;

        println!("Starting SBT");
        let mut sbt = Cmd
            .run_script(Sbt.lookup()?)?
            .current_dir(&paths.repo_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // .group_spawn()?;
            .spawn()?;


        let stdout = std::mem::take(&mut sbt.stdout).unwrap();
        let mut stdin = std::mem::take(&mut sbt.stdin).unwrap();
        let stderr = std::mem::take(&mut sbt.stderr).unwrap();

        let handle = tokio::task::spawn(process_lines(stdout, |line| {
            println!("OUT: {}", line.trim_end());
        }));
        let handle2 = tokio::task::spawn(process_lines(stderr, |line| {
            println!("ERR: {}", line.trim_end());
        }));

        stdin.write_all("bootstrap\n".as_bytes()).await?;

        tokio::time::sleep(Duration::from_secs(5)).await;
        // println!("Closing STDIN");
        // drop(stdin);
        // println!("Killing SBT");
        // sbt.kill().await?;
        println!("Waiting for OUT");
        handle.await??;
        println!("Waiting for ERR");
        handle2.await??;
        // sbt.wait().await?;

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn copy_file_js() -> Result {
        let paths = Paths::new(r"H:\NBO\enso")?;
        ide_ci::io::copy_to(
            paths.target.join("scala-parser.js"),
            paths.target.join("parser-upload"),
        )?;


        let schema_dir =
            paths.repo_root.join_many(["engine", "language-server", "src", "main", "schema"]);
        ide_ci::io::copy_to(&schema_dir, paths.target.join("fbs-upload"))?;
        ide_ci::programs::SevenZip
            .pack(paths.target.join("fbs-upload/fbs-schema.zip"), once(schema_dir.join("*")))
            .await?;

        Ok(())
    }

    #[test]
    #[ignore]
    fn system() {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        dbg!(system.total_memory());
    }
}
