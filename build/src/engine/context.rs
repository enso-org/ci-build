use crate::prelude::*;
use anyhow::Context;
use ide_ci::env::Variable;
use platforms::TARGET_ARCH;
use platforms::TARGET_OS;
use sysinfo::SystemExt;

use crate::args;
use crate::args::Args;
use crate::args::WhatToDo;
use crate::engine::create_bundles;
use crate::engine::create_packages;
use crate::engine::deduce_versions;
use crate::engine::download_project_templates;
use crate::engine::env;
use crate::engine::BuildConfiguration;
use crate::engine::BuildMode;
use crate::engine::BuildOperation;
use crate::engine::BuiltArtifacts;
use crate::engine::ComponentPathExt;
use crate::engine::Operation;
use crate::engine::ReleaseCommand;
use crate::engine::ReleaseOperation;
use crate::engine::RunOperation;
use crate::engine::FLATC_VERSION;
use crate::engine::PARALLEL_ENSO_TESTS;
use crate::get_graal_version;
use crate::get_java_major_version;
use crate::paths::cache_directory;
use crate::paths::Paths;
use crate::retrieve_github_access_token;
use crate::setup_octocrab;

use crate::engine::sbt::verify_generated_package;
use crate::enso::BuiltEnso;
use crate::enso::IrCaches;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::goodies;
use ide_ci::goodies::graalvm;
use ide_ci::models::config::RepoContext;
use ide_ci::paths;
use ide_ci::platform::default_shell;
use ide_ci::program::with_cwd::WithCwd;
use ide_ci::programs::Flatc;
use ide_ci::programs::Git;
use ide_ci::programs::Sbt;
use ide_ci::run_in_ci;

pub struct RunContext {
    pub config:    BuildConfiguration,
    pub octocrab:  Octocrab,
    pub paths:     Paths,
    pub goodies:   GoodieDatabase,
    pub operation: Operation,
}

impl RunContext {
    pub async fn new(args: &Args) -> Result<Self> {
        // Get default build configuration for a given build kind.
        let config = BuildConfiguration::new(&args);
        let octocrab = setup_octocrab()?;
        let enso_root = args.target.clone();
        println!("Received target location: {}", enso_root.display());
        let enso_root = args.target.absolutize()?.to_path_buf();
        println!("Absolute target location: {}", enso_root.display());


        let operation = match &args.command {
            WhatToDo::Create(_) | WhatToDo::Upload(_) | WhatToDo::Publish(_) =>
                Operation::Release(ReleaseOperation::new(args)?),
            WhatToDo::Run(args::Run { command_pieces }) =>
                Operation::Run(RunOperation { command_pieces: command_pieces.clone() }),
            WhatToDo::Build(args::Build {}) => Operation::Build(BuildOperation {}),
        };

        let target_repo = if let Operation::Release(release_op) = &operation {
            Some(&release_op.repo)
        } else {
            None
        };
        let versions = deduce_versions(&octocrab, args.kind, target_repo, &enso_root).await?;
        versions.publish()?;
        println!("Target version: {versions:?}.");
        let paths = Paths::new_version(&enso_root, versions.version.clone())?;
        let goodies = GoodieDatabase::new()?;
        Ok(Self { config, octocrab, paths, goodies, operation })
    }

    pub async fn prepare_build_env(&self) -> Result {
        // Building native images with Graal on Windows requires Microsoft Visual C++ Build Tools
        // available in the environment. If it is not visible, we need to add it.
        if TARGET_OS == OS::Windows && ide_ci::programs::vs::Cl.lookup().is_err() {
            ide_ci::programs::vs::apply_dev_environment().await?;
        }

        // Setup SBT
        self.goodies.require(&goodies::sbt::Sbt).await?;
        ide_ci::programs::Sbt.require_present().await?;

        // Other programs.
        ide_ci::programs::Git::default().require_present().await?;
        ide_ci::programs::Go.require_present().await?;
        ide_ci::programs::Cargo.require_present().await?;
        ide_ci::programs::Node.require_present().await?;
        ide_ci::programs::Npm.require_present().await?;

        // Setup Conda Environment
        // Install FlatBuffers Compiler
        // If it is not available, we require conda to install it. We should not require conda in
        // other scenarios.
        // TODO: After flatc version is bumped, it should be possible to get it without `conda`.
        //       See: https://www.pivotaltracker.com/story/show/180303547
        if let Err(e) = Flatc.require_present_at(&FLATC_VERSION).await {
            println!("Cannot find expected flatc: {}", e);
            // GitHub-hosted runner has `conda` on PATH but not things installed by it.
            // It provides `CONDA` variable pointing to the relevant location.
            if let Some(conda_path) = std::env::var_os("CONDA").map(PathBuf::from) {
                ide_ci::env::prepend_to_path(conda_path.join("bin"))?;
                if TARGET_OS == OS::Windows {
                    // Not sure if it documented anywhere, but this is where installed `flatc`
                    // appears on Windows.
                    ide_ci::env::prepend_to_path(conda_path.join("Library").join("bin"))?;
                }
            }

            ide_ci::programs::Conda
                .call_args(["install", "-y", "--freeze-installed", "flatbuffers=1.12.0"])
                .await?;
            ide_ci::programs::Flatc.lookup()?;
        }

        let _ = self.paths.emit_env_to_actions(); // Ignore error: we might not be run on CI.
        println!("Build configuration: {:#?}", self.config);

        // Setup Tests on Windows
        if TARGET_OS == OS::Windows {
            env::CiTestTimeFactor.set(&2);
            env::CiFlakyTestEnable.set(&true);
        }

        // if TARGET_OS == OS::Linux {
        //     let musl = ide_ci::goodies::musl::Musl;
        //     goodies.require(&musl).await?;
        // }

        let build_sbt_content = std::fs::read_to_string(self.paths.build_sbt())?;
        // Setup GraalVM
        let graalvm = graalvm::GraalVM {
            client:        &self.octocrab,
            graal_version: get_graal_version(&build_sbt_content)?,
            java_version:  get_java_major_version(&build_sbt_content)?,
            os:            TARGET_OS,
            arch:          TARGET_ARCH,
        };
        self.goodies.require(&graalvm).await?;
        graalvm::Gu.require_present().await?;
        graalvm::Gu.cmd()?.args(["install", "native-image"]).status().await?.exit_ok()?;
        Ok(())
    }

    pub async fn create_release(&self, repo: &RepoContext) -> Result {
        let versions = &self.paths.triple.versions;

        let commit = ide_ci::actions::env::Sha.fetch()?;
        let latest_changelog_body =
            crate::changelog::retrieve_unreleased_release_notes(self.paths.changelog())?;

        println!("Preparing release {} for commit {}", versions.version, commit);
        let release = repo
            .repos(&self.octocrab)
            .releases()
            .create(&versions.tag())
            .target_commitish(&commit)
            .name(&versions.pretty_name())
            .body(&latest_changelog_body.contents)
            .prerelease(true)
            .draft(true)
            .send()
            .await?;

        crate::env::ReleaseId.emit(&release.id)?;
        Ok(())
    }

    pub async fn publish_release(&self, repo: &RepoContext) -> Result {
        let release_id = crate::env::ReleaseId.fetch()?;
        println!("Looking for release with id {release_id} on github.");
        let release = repo.repos(&self.octocrab).releases().get_by_id(release_id).await?;
        println!("Found the target release, will publish it.");
        repo.repos(&self.octocrab).releases().update(release.id.0).draft(false).send().await?;
        iprintln!("Done. Release URL: {release.url}");

        self.paths.download_edition_file_artifact().await?;
        println!("Updating edition in the AWS S3.");
        crate::aws::update_manifest(repo, &self.paths).await?;
        Ok(())
    }

    pub async fn build(&self) -> Result<BuiltArtifacts> {
        self.prepare_build_env().await?;
        if ide_ci::run_in_ci() {
            // On CI we remove IR caches. They might contain invalid or outdated data, as are using
            // engine version as part of the key. As such, any change made to engine that does not
            // change its version might break the caches.
            // See (private): https://discord.com/channels/401396655599124480/407883082310352928/939618590158630922
            ide_ci::io::remove_dir_if_exists(cache_directory())?;
        }

        let git = Git::new(&self.paths.repo_root);
        if self.config.clean_repo {
            git.clean_xfd().await?;
            let lib_src = PathBuf::from_iter(["distribution", "lib"]);
            git.args(["checkout"])?.arg(lib_src).run_ok().await?;
        }

        // Install Dependencies of the Simple Library Server
        ide_ci::programs::Npm
            .install(self.paths.repo_root.join_many(["tools", "simple-library-server"]))?
            .run_ok()
            .await?;

        // Download Project Template Files
        let client = reqwest::Client::new();
        download_project_templates(client.clone(), self.paths.repo_root.clone()).await?;

        let sbt = WithCwd::new(Sbt, &self.paths.repo_root);

        let mut system = sysinfo::System::new();
        system.refresh_memory();
        dbg!(system.total_memory());
        dbg!(system.available_memory());
        dbg!(system.used_memory());
        dbg!(system.free_memory());

        // Build packages.
        println!("Bootstrapping Enso project.");
        sbt.call_arg("bootstrap").await?;

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

            if self.config.benchmark_compilation {
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

            if self.config.benchmark_compilation {
                // Check Runtime Benchmark Compilation
                sbt.call_arg("runtime/clean; runtime/Benchmark/compile").await?;

                // Check Language Server Benchmark Compilation
                sbt.call_arg("runtime/clean; language-server/Benchmark/compile").await?;

                // Check Searcher Benchmark Compilation
                sbt.call_arg("searcher/Benchmark/compile").await?;
            }
        }
        if self.config.test_scala {
            // Test Enso
            sbt.call_arg("set Global / parallelExecution := false; runtime/clean; compile; test")
                .await?;
        }

        // === Build Distribution ===
        // Build the Project Manager Native Image
        // FIXME looks like a copy-paste error

        if self.config.mode == BuildMode::Development {
            // docs-generator fails on Windows because it can't understand non-Unix-style paths.
            if TARGET_OS != OS::Windows {
                // Build the docs from standard library sources.
                sbt.call_arg("docs-generator/run").await?;
            }
        }

        if self.config.build_js_parser {
            // Build the Parser JS Bundle
            // TODO do once across the build
            // The builds are run on 3 platforms, but
            // Flatbuffer schemas are platform agnostic, so they just need to be
            // uploaded from one of the runners.
            sbt.call_arg("syntaxJS/fullOptJS").await?;
            ide_ci::io::copy_to(
                self.paths.target.join("scala-parser.js"),
                self.paths.target.join("parser-upload"),
            )?;
        }


        let enso = BuiltEnso { paths: self.paths.clone() };
        if self.config.test_standard_library {
            // Prepare Engine Test Environment
            if let Ok(gdoc_key) = std::env::var("GDOC_KEY") {
                let google_api_test_data_dir =
                    self.paths.repo_root.join("test").join("Google_Api_Test").join("data");
                ide_ci::io::create_dir_if_missing(&google_api_test_data_dir)?;
                std::fs::write(google_api_test_data_dir.join("secret.json"), &gdoc_key)?;
            }
            enso.run_tests(IrCaches::No, PARALLEL_ENSO_TESTS).await?;
        }

        let std_libs = self.paths.engine.dir.join("lib").join("Standard");
        // Compile the Standard Libraries (Unix)
        println!("Compiling standard libraries under {}", std_libs.display());
        for entry in std_libs.read_dir()? {
            let entry = entry?;
            let target = entry.path().join(self.paths.version().to_string());
            enso.compile_lib(target)?.run_ok().await?;
        }

        if self.config.test_standard_library {
            enso.run_tests(IrCaches::Yes, PARALLEL_ENSO_TESTS).await?;
        }


        // Verify License Packages in Distributions
        // FIXME apparently this does not work on Windows due to some CRLF issues?
        if self.config.mode == BuildMode::NightlyRelease && TARGET_OS != OS::Windows {
            /*  refversion=${{ env.ENSO_VERSION }}
                binversion=${{ env.DIST_VERSION }}
                engineversion=$(${{ env.ENGINE_DIST_DIR }}/bin/enso --version --json | jq -r '.version')
                test $binversion = $refversion || (echo "Tag version $refversion and the launcher version $binversion do not match" && false)
                test $engineversion = $refversion || (echo "Tag version $refversion and the engine version $engineversion do not match" && false)
            */



            verify_generated_package(&sbt, "engine", &self.paths.engine.dir).await?;
            verify_generated_package(&sbt, "launcher", &self.paths.launcher.dir).await?;
            verify_generated_package(&sbt, "project-manager", &self.paths.project_manager.dir)
                .await?;
            for libname in ["Base", "Table", "Image", "Database"] {
                verify_generated_package(
                    &sbt,
                    libname,
                    self.paths
                        .engine
                        .dir
                        .join_many(["lib", "Standard"])
                        .join(libname)
                        .join(self.paths.version().to_string()),
                )
                .await?;
            }
        }

        // Compress the built artifacts for upload
        // The artifacts are compressed before upload to work around an error with long path
        // handling in the upload-artifact action on Windows. See: https://github.com/actions/upload-artifact/issues/240
        self.paths.engine.pack().await?;
        let schema_dir =
            self.paths.repo_root.join_many(["engine", "language-server", "src", "main", "schema"]);
        let schema_files = schema_dir.read_dir()?.map(|e| e.map(|e| e.path())).collect_result()?;
        ide_ci::archive::create(self.paths.target.join("fbs-upload/fbs-schema.zip"), schema_files)
            .await?;

        if TARGET_OS == OS::Linux && run_in_ci() {
            self.paths.upload_edition_file_artifact().await?;
        }

        if self.config.build_bundles {
            // Launcher bundle
            let packages = create_packages(&self.paths).await?;
            let bundles = create_bundles(&self.paths).await?;
            Ok(BuiltArtifacts { packages, bundles })
        } else {
            Ok(default())
        }
    }

    pub async fn execute(&self) -> Result {
        match &self.operation {
            Operation::Release(ReleaseOperation { command, repo }) => match command {
                ReleaseCommand::Create => {
                    self.create_release(repo).await?;
                }
                ReleaseCommand::Publish => {
                    self.publish_release(repo).await?;
                }
                ReleaseCommand::Upload => {
                    let artifacts = self.build().await?;

                    // Make packages.
                    let release_id = crate::env::ReleaseId.fetch()?;
                    let repo_handler = repo.repos(&self.octocrab);

                    let releases_handler = repo_handler.releases();
                    let release = releases_handler.get_by_id(release_id).await.context(format!(
                        "Failed to find release by id `{release_id}` in `{repo}`."
                    ))?;

                    let client = ide_ci::github::create_client(retrieve_github_access_token()?)?;
                    for package in artifacts.packages {
                        ide_ci::github::release::upload_asset(repo, &client, release.id, package)
                            .await?;
                    }
                    for bundle in artifacts.bundles {
                        ide_ci::github::release::upload_asset(repo, &client, release.id, bundle)
                            .await?;
                    }
                }
            },
            Operation::Run(run) => {
                // Build environment preparations.
                self.prepare_build_env().await?;
                let mut run = run.command_pieces.iter();
                if let Some(program) = run.next() {
                    println!("Spawning program {}.", program.to_str().unwrap());
                    tokio::process::Command::new(program)
                        .args(run)
                        .current_dir(&self.paths.repo_root)
                        .spawn()?
                        .wait()
                        .await?
                        .exit_ok()?;
                } else {
                    println!("Spawning default shell.");
                    default_shell()
                        .run_shell()?
                        .current_dir(&self.paths.repo_root)
                        .run_ok()
                        .await?;
                }
            }
            Operation::Build(_) => {
                self.build().boxed().await?;
            }
        };

        Ok(())
    }
}
