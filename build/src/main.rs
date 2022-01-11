#![feature(result_cloned)]
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

use enso_build::paths::Paths;
use ide_ci::extensions::path::PathExt;
use ide_ci::future::AsyncPolicy;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::goodies::graalvm;
use ide_ci::goodies::sbt;
use ide_ci::program::with_cwd::WithCwd;
use ide_ci::programs::git::Git;
use ide_ci::programs::Docker;
use ide_ci::programs::Go;
use ide_ci::programs::Sbt;
use ide_ci::programs::SevenZip;
use octocrab::OctocrabBuilder;
use platforms::TARGET_ARCH;
use platforms::TARGET_OS;
use std::process::Stdio;
use sysinfo::SystemExt;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::process::Child;

const FLATC_VERSION: Version = Version::new(1, 12, 0);
const GRAAL_VERSION: Version = Version::new(21, 1, 0);
const GRAAL_JAVA_VERSION: graalvm::JavaVersion = graalvm::JavaVersion::Java11;

const PARALLEL_ENSO_TESTS: AsyncPolicy = AsyncPolicy::Sequential;

const POSTGRESQL_PORT: u16 = 5432;

#[cfg(target_os = "linux")]
const LIBRARIES_TO_TEST: [&str; 6] =
    ["Tests", "Table_Tests", "Database_Tests", "Geo_Tests", "Visualization_Tests", "Image_Tests"];

// Test postgres only on Linux
#[cfg(not(target_os = "linux"))]
const LIBRARIES_TO_TEST: [&str; 5] =
    ["Tests", "Table_Tests", "Geo_Tests", "Visualization_Tests", "Image_Tests"];

#[derive(Clone, Debug)]
pub struct BootstrapParameters {
    pub repo_root: PathBuf,
}

#[derive(Copy, Clone, Debug)]
pub enum IrCaches {
    Yes,
    No,
}

impl IrCaches {
    pub fn flag(self) -> &'static str {
        match self {
            IrCaches::Yes => "--ir-caches",
            IrCaches::No => "--no-ir-caches",
        }
    }
}

impl AsRef<OsStr> for IrCaches {
    fn as_ref(&self) -> &OsStr {
        self.flag().as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct BuiltEnso {
    paths: Paths,
}

impl BuiltEnso {
    pub fn wrapper_script_path(&self) -> PathBuf {
        self.paths.engine.dir.join("bin").join("enso")
    }

    pub fn test_path(&self, test_project_name: &str) -> PathBuf {
        self.paths.repo_root.join("test").join(test_project_name)
    }

    pub fn run_test(&self, test: impl AsRef<str>, ir_caches: IrCaches) -> Result<Command> {
        let mut command = self.cmd()?;
        command
            .arg(ir_caches)
            .arg("--run")
            .arg(self.test_path(test.as_ref()))
            .env("ENSO_DATABASE_TEST_DB_NAME", "enso_test_db")
            .env("ENSO_DATABASE_TEST_HOST", "127.0.0.1")
            .env("ENSO_DATABASE_TEST_DB_USER", "enso_test_user")
            .env("ENSO_DATABASE_TEST_DB_PASSWORD", "enso_test_password");
        Ok(command)
    }

    pub fn compile_lib(&self, target: impl AsRef<Path>) -> Result<Command> {
        let mut command = self.cmd()?;
        command
            .arg(IrCaches::Yes)
            .args(["--no-compile-dependencies", "--no-global-cache", "--compile"])
            .arg(target.as_ref());
        Ok(command)
    }
}

#[async_trait]
impl Program for BuiltEnso {
    fn executable_name() -> &'static str {
        ide_ci::platform::DefaultShell::executable_name()
    }

    fn cmd(&self) -> Result<Command> {
        ide_ci::platform::default_shell().run_script(self.wrapper_script_path())
    }

    async fn version_string(&self) -> Result<String> {
        let output = self.cmd()?.args(["version", "--json", "--only-launcher"]).output().await?;
        output.status.exit_ok().map_err(|e| {
            anyhow!(
                "Failed to get version: {}. \nStdout: {}\nStderr: {}",
                e,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
        String::from_utf8(output.stdout).anyhow_err()
    }

    async fn version(&self) -> Result<Version> {
        #[derive(Clone, Debug, Deserialize)]
        struct VersionInfo {
            version: Version,
        }

        let stdout = self.version_string().await?;
        let version = serde_json::from_str::<VersionInfo>(&stdout)?;
        Ok(version.version)
    }
}

/// Build, test and packave Enso Engine.
#[derive(Clone, Debug, FromArgs)]
pub struct Args {
    /// build a nightly release
    #[argh(option, default = "false")]
    pub nightly:    bool,
    /// path to the Enso Engine repository
    #[argh(positional)]
    pub repository: PathBuf,
}

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

pub async fn run_tests(paths: &Paths, ir_caches: IrCaches, async_policy: AsyncPolicy) -> Result {
    let built_enso = BuiltEnso { paths: paths.clone() };

    // Prepare Engine Test Environment
    if let Ok(gdoc_key) = std::env::var("GDOC_KEY") {
        let google_api_test_data_dir =
            paths.repo_root.join("test").join("Google_Api_Test").join("data");
        ide_ci::io::create_dir_if_missing(&google_api_test_data_dir)?;
        std::fs::write(google_api_test_data_dir.join("secret.json"), &gdoc_key)?;
    }

    let _httpbin = get_and_spawn_httpbin().await?;
    let _postgres = match TARGET_OS {
        OS::Linux => Some(
            Postgresql::start("enso_test_db", "enso_test_user", "enso_test_password", "latest")
                .await?,
        ),
        _ => None,
    };

    let futures = LIBRARIES_TO_TEST.map(ToString::to_string).map(|test| {
        let command = built_enso.run_test(test, ir_caches);
        async move { command?.run_ok().await }
    });

    let _result = ide_ci::future::try_join_all(futures, async_policy).await?;

    // We need to join all the test tasks here, as they require postgres and httpbin alive.
    // Could share them with Arc but then scenario of multiple test runs being run in parallel
    // should be handled, e.g. avoiding port collisions.
    Ok(())
}

async fn expect_flatc(version: &Version) -> Result {
    let found_version = ide_ci::programs::Flatc.version().await?;
    if &found_version == version {
        Ok(())
    } else {
        bail!("Failed to find flatc {}. Found version: {}", version, found_version)
    }
}

pub fn setup_octocrab() -> Result<Octocrab> {
    let builder = match (OctocrabBuilder::new(), std::env::var("GITHUB_TOKEN")) {
        (builder, Ok(github_token)) => builder.personal_token(github_token),
        (builder, _) => builder,
    };
    builder.build().anyhow_err()
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
}

const LOCAL: BuildConfiguration = BuildConfiguration {
    clean_repo:            true,
    mode:                  BuildMode::Development,
    test_scala:            true,
    test_standard_library: true,
    benchmark_compilation: true,
};

const NIGHTLY: BuildConfiguration = BuildConfiguration {
    clean_repo:            true,
    mode:                  BuildMode::NightlyRelease,
    test_scala:            false,
    test_standard_library: false,
    benchmark_compilation: false,
};


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Initial environment:");
    for (key, value) in std::env::vars() {
        iprintln!("\t{key}={value}");
    }

    let args: Args = argh::from_env();
    let octocrab = setup_octocrab()?;
    let enso_root = args.repository.clone();
    println!("Repository location: {}", enso_root.display());

    let paths = if args.nightly {
        let versions = enso_build::preflight_check::prepare_nightly(&octocrab, &enso_root).await?;
        Paths::new_version(&enso_root, versions.engine)?
    } else {
        Paths::new(&enso_root)?
    };


    let config = if args.nightly { NIGHTLY } else { LOCAL };

    if config.clean_repo {
        Git::new(&paths.repo_root).clean_xfd().await?;
    }

    let _ = paths.emit_to_actions(); // Ignore error: we might not be run on CI.
    println!("Build configuration: {:#?}", config);

    let goodies = GoodieDatabase::new()?;
    let client = reqwest::Client::new();

    // Building native images with Graal on Windows requires Microsoft Visual C++ Build Tools
    // available in the environment. If it is not visible, we need to add it.
    if TARGET_OS == OS::Windows && ide_ci::programs::vs::Cl.lookup().is_err() {
        ide_ci::programs::vs::apply_dev_environment().await?;
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
    if let Err(e) = expect_flatc(&FLATC_VERSION).await {
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
    if true {
        println!("Bootstrapping Enso project.");
        sbt.call_arg("bootstrap").await?;

        println!("Verifying the Stdlib Version.");
        match config.mode {
            BuildMode::Development => {
                sbt.call_arg("stdlib-version-updater/run check").await?;
            }
            BuildMode::NightlyRelease => {
                sbt.call_arg("stdlib-version-updater/run update --no-format").await?;
                sbt.call_arg("verifyLicensePackages").await?;
            }
        };
        // Compile
        sbt.call_arg("compile").await?;

        // Setup Tests on Windows
        if TARGET_OS == OS::Windows {
            std::env::set_var("CI_TEST_TIMEFACTOR", "2");
            std::env::set_var("CI_TEST_FLAKY_ENABLE", "true");
        }

        // Build the Runner & Runtime Uberjars
        sbt.call_arg("runtime/clean; engine-runner/assembly").await?;

        // Build the Launcher Native Image
        sbt.call_arg("launcher/assembly").await?;
        sbt.call_args(["--mem", "1536", "launcher/buildNativeImage"]).await?;

        // Build the PM Native Image
        sbt.call_arg("project-manager/assembly").await?;
        sbt.call_args(["--mem", "1536", "project-manager/buildNativeImage"]).await?;

        if config.test_scala {
            // Test Enso
            sbt.call_arg("set Global / parallelExecution := false; runtime/clean; compile; test")
                .await
                .ok(); // FIXME
        }

        if config.benchmark_compilation {
            // Check Runtime Benchmark Compilation
            sbt.call_arg("runtime/clean; runtime/Benchmark/compile").await?;

            // Check Language Server Benchmark Compilation
            sbt.call_arg("runtime/clean; language-server/Benchmark/compile").await?;

            // Check Searcher Benchmark Compilation
            sbt.call_arg("searcher/Benchmark/compile").await?;
        }

        // === Build Distribution ===
        // Build the Project Manager Native Image
        // FIXME looks like a copy-paste error

        if config.mode == BuildMode::Development {
            sbt.call_arg("project-manager/assembly").await?;
            sbt.call_args(["--mem", "1536", "launcher/buildNativeImage"]).await?;

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

            // docs-generator fails on Windows because it can't understand non-Unix-style paths.
            if TARGET_OS != OS::Windows {
                // Build the docs from standard library sources.
                sbt.call_arg("docs-generator/run").await?;
            }
        }

        // Prepare Launcher Distribution
        sbt.call_arg("buildLauncherDistribution").await?;

        // Prepare Engine Distribution
        sbt.call_arg("runtime/clean; buildEngineDistribution").await?;

        // Prepare Project Manager Distribution

        if config.mode == BuildMode::NightlyRelease {
            // Prepare GraalVM Distribution
            sbt.call_arg("buildGraalDistribution").await?;
        }
    }


    let enso = BuiltEnso { paths: paths.clone() };


    // Install Graalpython & FastR
    if TARGET_OS != OS::Windows {
        graalvm::Gu.call_args(["install", "python", "r"]).await?;
    }

    if config.test_standard_library {
        // Prepare Engine Test Environment
        if let Ok(gdoc_key) = std::env::var("GDOC_KEY") {
            let google_api_test_data_dir =
                enso_root.join("test").join("Google_Api_Test").join("data");
            ide_ci::io::create_dir_if_missing(&google_api_test_data_dir)?;
            std::fs::write(google_api_test_data_dir.join("secret.json"), &gdoc_key)?;
        }

        run_tests(&paths, IrCaches::No, PARALLEL_ENSO_TESTS).await?;

        let std_libs = paths.engine.dir.join("lib").join("Standard");
        // Compile the Standard Libraries (Unix)
        for entry in std_libs.read_dir()? {
            let entry = entry?;
            let target = entry.path().join(paths.version.to_string());
            enso.compile_lib(target)?.run_ok().await?;
        }

        run_tests(&paths, IrCaches::Yes, PARALLEL_ENSO_TESTS).await?;
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
                .arg("enso/verifyGeneratedPackage")
                .arg(package)
                .arg(path.as_ref().join("THIRD-PARTY"))
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
                paths.engine.dir.join_many(["lib", "Standard"]).join(libname),
            )
            .await?
        }
    }

    // Compress the built artifacts for upload
    let output_archive = paths.engine.root.join(&paths.engine.name).append_extension("zip");
    // The artifacts are compressed before upload to work around an error with long path handling in
    // the upload-artifact action on Windows. See: https://github.com/actions/upload-artifact/issues/240
    SevenZip.pack_cmd(&output_archive, once("*"))?.current_dir(&paths.engine.root).run_ok().await?;

    ide_ci::io::copy_to(paths.target.join("scala-parser.js"), paths.target.join("parser-upload"))?;

    let schema_dir =
        paths.repo_root.join_many(["engine", "language-server", "src", "main", "schema"]);
    ide_ci::io::copy_to(&schema_dir, paths.target.join("fbs-upload"))?;
    ide_ci::programs::SevenZip
        .pack(paths.target.join("fbs-upload/fbs-schema.zip"), once(schema_dir.join("*")))
        .await?;

    Ok(())
}

/// Retrieve input from asynchronous reader line by line and feed them into the given function.
pub async fn process_lines<R: AsyncRead + Unpin>(reader: R, f: impl Fn(String)) -> Result<R> {
    println!("Started line processor.");
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    while reader.read_line(&mut line).await? != 0 {
        f(std::mem::take(&mut line));
    }
    Ok(reader.into_inner())
}

pub async fn process_lines_until<R: AsyncRead + Unpin>(
    reader: R,
    f: &impl Fn(&str) -> bool,
) -> Result<R> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        let bytes_read = reader.read_line(&mut line).await?;
        ensure!(bytes_read != 0, "Postgresql container closed without being ready!");
        if f(&line) {
            break;
        }
        line.clear();
    }
    Ok(reader.into_inner())
}

pub struct PostgresContainer {
    _docker_run: Child,
    name:        String,
}

impl Drop for PostgresContainer {
    fn drop(&mut self) {
        println!("Will kill the postgres container");
        let kill_fut = Docker.call_args(["kill", self.name.as_str()]);
        if let Err(e) = futures::executor::block_on(kill_fut) {
            println!("Failed to kill the Postgres container named {}: {}", self.name, e);
        } else {
            println!("Postgres container killed.");
        }
    }
}

pub struct Postgresql;

impl Postgresql {
    pub async fn start(
        dbname: &str,
        user: &str,
        password: &str,
        version: &str,
    ) -> Result<PostgresContainer> {
        let name = Uuid::new_v4().to_string();
        let env =
            [("POSTGRES_DB", dbname), ("POSTGRES_USER", user), ("POSTGRES_PASSWORD", password)];


        let mut cmd = Docker.cmd()?;
        cmd.arg("run");
        for (var, val) in env {
            cmd.arg("-e").arg(format!("{}={}", var, val));
        }
        cmd.arg("--sig-proxy=true");
        cmd.arg("-p").arg(format!("{}:{}", POSTGRESQL_PORT, POSTGRESQL_PORT));
        cmd.arg("--name").arg(&name);
        cmd.stderr(Stdio::piped());
        cmd.arg(format!("postgres:{}", version));
        cmd.kill_on_drop(true);
        let mut child = cmd.spawn().anyhow_err()?;
        let stderr = child
            .stderr
            .ok_or_else(|| anyhow!("Failed to access standard output of the spawned process!"))?;
        let check_line = |line: &str| {
            println!("ERR: {}", line);
            line.contains("database system is ready to accept connections")
        };
        let stderr = process_lines_until(stderr, &check_line).await?;
        child.stderr = Some(stderr);
        Ok(PostgresContainer { _docker_run: child, name })
        // bail!("Error reading Postgres service standard output.")
    }
}

pub async fn fix_duplicated_env_var(var_name: impl AsRef<OsStr>) -> Result {
    let var_name = var_name.as_ref();

    let mut paths = indexmap::IndexSet::new();
    while let Ok(path) = std::env::var(var_name) {
        paths.extend(std::env::split_paths(&path));
        std::env::remove_var(var_name);
    }
    std::env::set_var(var_name, std::env::join_paths(paths)?);
    Ok(())
}


pub async fn get_and_spawn_httpbin() -> Result<Child> {
    Go.call_args(["get", "-v", "github.com/ahmetb/go-httpbin/cmd/httpbin"]).await?;
    let gopath = String::from_utf8(
        Go.cmd()?.args(["env", "GOPATH"]).stdout(Stdio::piped()).output().await?.stdout,
    )?;
    let gopath = gopath.trim();
    let gopath = PathBuf::from(gopath); // be careful of trailing newline!
    let program = gopath.join("bin").join("httpbin");
    println!("Will spawn {}", program.display());
    Command::new(program).args(["-host", ":8080"]).kill_on_drop(true).spawn().anyhow_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    use ide_ci::extensions::path::PathExt;
    use ide_ci::programs::git::Git;
    use ide_ci::programs::Cmd;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::time::sleep;

    #[tokio::test]
    async fn start_postgres() -> Result {
        let child = Postgresql::start("test", "test", "test", "latest").await?;
        sleep(Duration::from_secs(5)).await;
        drop(child);
        Ok(())
    }

    #[tokio::test]
    async fn spawn_httpbin() -> Result {
        let mut httpbin = get_and_spawn_httpbin().await?;
        sleep(Duration::from_secs(15)).await;
        httpbin.kill().await?;
        Ok(())
    }

    #[tokio::test]
    async fn download_stuff() -> Result {
        download_project_templates(reqwest::Client::new(), PathBuf::from("C:/temp")).await?;
        Ok(())
    }
    #[tokio::test]
    async fn test_paths() -> Result {
        let paths = Paths::new(r"H:\NBO\enso")?;
        let mut output_archive = paths.engine.dir.join(&paths.engine.name);
        // The artifacts are compressed before upload to work around an error with long path
        // handling in the upload-artifact action on Windows. See: https://github.com/actions/upload-artifact/issues/240
        output_archive = output_archive.append_extension("zip");
        println!("{}", output_archive.display());
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
    fn system() {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        dbg!(system.total_memory());
    }
}
